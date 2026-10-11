//! Pure HTTP model snapshot download (Hugging Face / HF Mirror / ModelScope).
//! Supports Range resume and SHA-256 integrity checks. No CLI / hf-hub.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::config::DownloadSource;
use crate::engine::dir_size;

const UA: &str = "H3.UI";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// Listing / metadata only — downloads intentionally have no total timeout
/// (multi‑GiB weights can take longer than any fixed request timeout).
const META_TIMEOUT: Duration = Duration::from_secs(120);
const CHUNK: usize = 256 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DownloadStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub task_id: String,
    pub status: DownloadStatus,
    pub progress: f64,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub files_done: u32,
    pub files_total: u32,
    pub current_file: Option<String>,
    pub speed_bps: f64,
    pub message: String,
    pub error: Option<String>,
    pub source: String,
    pub repo: String,
}

pub struct DownloadManager {
    current: Mutex<Option<ActiveDownload>>,
}

struct ActiveDownload {
    task_id: String,
    cancel: Arc<AtomicBool>,
    progress: Arc<Mutex<DownloadProgress>>,
}

#[derive(Debug, Clone)]
struct RemoteFile {
    path: String,
    size: Option<u64>,
    /// Lowercase hex SHA-256 of the final object (not the LFS pointer).
    sha256: Option<String>,
}

impl DownloadManager {
    pub fn new() -> Self {
        Self {
            current: Mutex::new(None),
        }
    }

    pub fn current_progress(&self) -> Option<DownloadProgress> {
        self.current
            .lock()
            .as_ref()
            .map(|a| a.progress.lock().clone())
    }

    pub fn get_progress(&self, task_id: &str) -> Option<DownloadProgress> {
        let guard = self.current.lock();
        let a = guard.as_ref()?;
        if a.task_id != task_id {
            return None;
        }
        let snap = a.progress.lock().clone();
        Some(snap)
    }

    pub fn cancel(&self, task_id: &str) -> bool {
        let guard = self.current.lock();
        if let Some(a) = guard.as_ref() {
            if a.task_id == task_id {
                a.cancel.store(true, Ordering::SeqCst);
                return true;
            }
        }
        false
    }

    pub fn start(
        &self,
        source: DownloadSource,
        repo: String,
        dest: PathBuf,
    ) -> Result<(String, Arc<Mutex<DownloadProgress>>, Arc<AtomicBool>), String> {
        let mut guard = self.current.lock();
        if let Some(a) = guard.as_ref() {
            let st = a.progress.lock().status.clone();
            if matches!(st, DownloadStatus::Queued | DownloadStatus::Running) {
                return Err("a download is already in progress".into());
            }
        }
        let task_id = Uuid::new_v4().to_string();
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(Mutex::new(DownloadProgress {
            task_id: task_id.clone(),
            status: DownloadStatus::Queued,
            progress: 0.0,
            downloaded_bytes: 0,
            total_bytes: None,
            files_done: 0,
            files_total: 0,
            current_file: None,
            speed_bps: 0.0,
            message: "queued".into(),
            error: None,
            source: source.as_str().into(),
            repo: repo.clone(),
        }));
        *guard = Some(ActiveDownload {
            task_id: task_id.clone(),
            cancel: cancel.clone(),
            progress: progress.clone(),
        });

        let progress_bg = progress.clone();
        let cancel_bg = cancel.clone();
        std::thread::Builder::new()
            .name("h3-download".into())
            .spawn(move || {
                run_download(source, &repo, &dest, progress_bg, cancel_bg);
            })
            .map_err(|e| e.to_string())?;

        Ok((task_id, progress, cancel))
    }
}

fn http_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent(UA)
        .connect_timeout(CONNECT_TIMEOUT)
        // No overall timeout: body reads for large safetensors must not abort at 10m.
        .timeout(None)
        .redirect(reqwest::redirect::Policy::limited(16))
        .build()
        .map_err(|e| e.to_string())
}

fn run_download(
    source: DownloadSource,
    repo: &str,
    dest: &Path,
    progress: Arc<Mutex<DownloadProgress>>,
    cancel: Arc<AtomicBool>,
) {
    {
        let mut p = progress.lock();
        p.status = DownloadStatus::Running;
        p.message = format!("listing · {} · {repo}", source.as_str());
    }

    if let Err(e) = std::fs::create_dir_all(dest) {
        fail(&progress, e.to_string(), "failed to create model directory");
        return;
    }

    let client = match http_client() {
        Ok(c) => c,
        Err(e) => {
            fail(&progress, e, "http client");
            return;
        }
    };

    let files = match list_files(&client, source, repo) {
        Ok(f) => f,
        Err(e) => {
            fail(&progress, e, "list files");
            return;
        }
    };
    if files.is_empty() {
        fail(
            &progress,
            "remote returned an empty file list".into(),
            "list files",
        );
        return;
    }

    let total_bytes: Option<u64> = {
        let sizes: Vec<u64> = files.iter().filter_map(|f| f.size).collect();
        if sizes.len() == files.len() {
            Some(sizes.iter().sum())
        } else {
            None
        }
    };
    {
        let mut p = progress.lock();
        p.total_bytes = total_bytes;
        p.files_total = files.len() as u32;
        p.message = format!("listed {} files", files.len());
    }

    let dest_owned = dest.to_path_buf();
    let progress_poll = progress.clone();
    let cancel_poll = cancel.clone();
    let start = Instant::now();
    let poller = std::thread::spawn(move || {
        let mut last_bytes = 0u64;
        let mut last_t = Instant::now();
        while !cancel_poll.load(Ordering::SeqCst) {
            let bytes = dir_size(&dest_owned).unwrap_or(0);
            let now = Instant::now();
            let dt = now.duration_since(last_t).as_secs_f64().max(0.001);
            let speed = (bytes.saturating_sub(last_bytes) as f64) / dt;
            last_bytes = bytes;
            last_t = now;
            {
                let mut p = progress_poll.lock();
                if !matches!(p.status, DownloadStatus::Running) {
                    break;
                }
                p.downloaded_bytes = bytes;
                p.speed_bps = speed;
                // Prefer file-count progress — byte ratio stays ~0% for a long time
                // on a ~500 GiB repo and looks stuck.
                p.progress = file_progress(p.files_done, p.files_total, bytes, p.total_bytes);
            }
            std::thread::sleep(Duration::from_millis(400));
        }
    });

    let result = download_files(&client, source, repo, dest, &files, &progress, &cancel);

    let _ = poller.join();
    let elapsed = start.elapsed();

    let mut p = progress.lock();
    match result {
        Ok(()) => {
            if cancel.load(Ordering::SeqCst) {
                p.status = DownloadStatus::Cancelled;
                p.message = "cancelled".into();
            } else {
                let bytes = dir_size(dest).unwrap_or(0);
                p.status = DownloadStatus::Completed;
                p.downloaded_bytes = bytes;
                p.progress = 100.0;
                p.message = format!(
                    "completed in {:.0}s ({:.2} GiB)",
                    elapsed.as_secs_f64(),
                    bytes as f64 / (1024.0 * 1024.0 * 1024.0)
                );
            }
        }
        Err(e) => {
            if cancel.load(Ordering::SeqCst) || e == "cancelled" {
                p.status = DownloadStatus::Cancelled;
                p.message = "cancelled (partial kept for resume)".into();
                p.error = None;
            } else {
                p.status = DownloadStatus::Failed;
                p.error = Some(e);
                p.message = "download failed".into();
            }
        }
    }
}

fn fail(progress: &Arc<Mutex<DownloadProgress>>, error: String, message: &str) {
    let mut p = progress.lock();
    p.status = DownloadStatus::Failed;
    p.error = Some(error);
    p.message = message.into();
}

fn list_files(
    client: &reqwest::blocking::Client,
    source: DownloadSource,
    repo: &str,
) -> Result<Vec<RemoteFile>, String> {
    match source {
        DownloadSource::Huggingface | DownloadSource::HfMirror => {
            let endpoint = source
                .hf_endpoint()
                .unwrap_or("https://huggingface.co");
            list_hf_files(client, endpoint, repo)
        }
        DownloadSource::Modelscope => list_modelscope_files(client, repo),
    }
}

fn list_hf_files(
    client: &reqwest::blocking::Client,
    endpoint: &str,
    repo: &str,
) -> Result<Vec<RemoteFile>, String> {
    let mut files = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let mut url = format!(
            "{endpoint}/api/models/{repo}/tree/main?recursive=true&limit=1000"
        );
        if let Some(c) = &cursor {
            url.push_str("&cursor=");
            url.push_str(&percent_encode(c));
        }
        let mut req = client.get(&url).timeout(META_TIMEOUT);
        if let Ok(token) = std::env::var("HF_TOKEN") {
            if !token.is_empty() {
                req = req.bearer_auth(token);
            }
        }
        let resp = req.send().map_err(|e| format!("HF tree: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("HF tree HTTP {}", resp.status()));
        }
        let next_cursor = resp
            .headers()
            .get("x-next-cursor")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .or_else(|| {
                resp.headers()
                    .get(reqwest::header::LINK)
                    .and_then(|v| v.to_str().ok())
                    .and_then(parse_link_next_cursor)
            });
        let items: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
        let arr = items
            .as_array()
            .ok_or_else(|| "HF tree: expected JSON array".to_string())?;
        if arr.is_empty() {
            break;
        }
        for item in arr {
            if item.get("type").and_then(|t| t.as_str()) != Some("file") {
                continue;
            }
            let path = item
                .get("path")
                .and_then(|p| p.as_str())
                .ok_or_else(|| "HF tree: missing path".to_string())?
                .to_string();
            let size = item
                .pointer("/lfs/size")
                .and_then(|s| s.as_u64())
                .or_else(|| item.get("size").and_then(|s| s.as_u64()))
                .filter(|n| *n > 0);
            let sha256 = item
                .pointer("/lfs/oid")
                .and_then(|v| v.as_str())
                .and_then(normalize_sha256)
                .or_else(|| item.get("oid").and_then(|v| v.as_str()).and_then(normalize_sha256));
            files.push(RemoteFile {
                path,
                size,
                sha256,
            });
        }
        match next_cursor {
            Some(c) if !c.is_empty() => cursor = Some(c),
            _ => break,
        }
    }
    Ok(files)
}

fn parse_link_next_cursor(link: &str) -> Option<String> {
    for part in link.split(',') {
        if !part.contains("rel=\"next\"") && !part.contains("rel='next'") {
            continue;
        }
        let url = part.split(';').next()?.trim();
        let url = url.trim_start_matches('<').trim_end_matches('>');
        for kv in url.split(['?', '&']) {
            if let Some(v) = kv.strip_prefix("cursor=") {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn list_modelscope_files(
    client: &reqwest::blocking::Client,
    repo: &str,
) -> Result<Vec<RemoteFile>, String> {
    let url = format!(
        "https://www.modelscope.cn/api/v1/models/{repo}/repo/files?Revision=master&Recursive=true"
    );
    let resp = client
        .get(&url)
        .timeout(META_TIMEOUT)
        .send()
        .map_err(|e| format!("ModelScope list: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("ModelScope list HTTP {}", resp.status()));
    }
    let body: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
    if body.get("Success").and_then(|v| v.as_bool()) == Some(false) {
        return Err(format!(
            "ModelScope API: {}",
            body.get("Message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error")
        ));
    }
    let mut files = Vec::new();
    let arr = body
        .pointer("/Data/Files")
        .and_then(|v| v.as_array())
        .or_else(|| body.get("Files").and_then(|v| v.as_array()))
        .ok_or_else(|| "ModelScope list: missing Files".to_string())?;
    for item in arr {
        let ty = item.get("Type").and_then(|t| t.as_str()).unwrap_or("");
        if ty != "blob" && ty != "file" {
            continue;
        }
        let path = item
            .get("Path")
            .and_then(|p| p.as_str())
            .ok_or_else(|| "ModelScope list: missing Path".to_string())?
            .to_string();
        let size = item
            .get("Size")
            .and_then(|s| s.as_u64())
            .filter(|n| *n > 0);
        let sha256 = item
            .get("Sha256")
            .and_then(|v| v.as_str())
            .and_then(normalize_sha256);
        files.push(RemoteFile {
            path,
            size,
            sha256,
        });
    }
    Ok(files)
}

fn normalize_sha256(raw: &str) -> Option<String> {
    let s = raw
        .trim()
        .strip_prefix("sha256:")
        .unwrap_or(raw.trim())
        .to_ascii_lowercase();
    if s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(s)
    } else {
        None
    }
}

fn file_progress(
    files_done: u32,
    files_total: u32,
    bytes: u64,
    total_bytes: Option<u64>,
) -> f64 {
    if files_total == 0 {
        return 0.0;
    }
    let file_pct = (files_done as f64 / files_total as f64) * 100.0;
    if let Some(total) = total_bytes.filter(|t| *t > 0) {
        let byte_pct = (bytes as f64 / total as f64) * 100.0;
        // Blend: file index is the main signal early on; bytes refine later.
        return (file_pct * 0.7 + byte_pct * 0.3).min(99.5);
    }
    file_pct.min(99.5)
}

fn touch_file_progress(
    progress: &Arc<Mutex<DownloadProgress>>,
    done: u32,
    total: u32,
    path: &str,
    phase: &str,
) {
    let mut p = progress.lock();
    p.files_done = done;
    p.files_total = total;
    p.current_file = Some(path.to_string());
    p.message = format!("{phase} · {done}/{total} · {path}");
    p.progress = file_progress(done, total, p.downloaded_bytes, p.total_bytes);
}

fn download_files(
    client: &reqwest::blocking::Client,
    source: DownloadSource,
    repo: &str,
    dest: &Path,
    files: &[RemoteFile],
    progress: &Arc<Mutex<DownloadProgress>>,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let total = files.len() as u32;
    let mut done = 0u32;
    for file in files {
        if cancel.load(Ordering::SeqCst) {
            return Err("cancelled".into());
        }
        touch_file_progress(progress, done, total, &file.path, "fetch");
        let target = dest.join(&file.path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        match verify_complete(&target, file)? {
            VerifyResult::Ok => {
                done += 1;
                touch_file_progress(progress, done, total, &file.path, "skip");
                continue;
            }
            VerifyResult::Missing | VerifyResult::Incomplete => {}
            VerifyResult::Corrupt => {
                let _ = std::fs::remove_file(&target);
                let _ = std::fs::remove_file(part_path(&target));
                let _ = std::fs::remove_file(ok_path(&target));
            }
        }

        touch_file_progress(progress, done, total, &file.path, "download");
        let urls = file_urls(source, repo, &file.path);
        let mut last_err = String::new();
        let mut ok = false;
        for url in &urls {
            match download_one(client, url, &target, file, cancel) {
                Ok(()) => {
                    ok = true;
                    break;
                }
                Err(e) if e == "cancelled" => return Err(e),
                Err(e) => last_err = e,
            }
        }
        if !ok {
            return Err(last_err);
        }

        if file.sha256.is_some() {
            touch_file_progress(progress, done, total, &file.path, "verify");
        }
        match verify_complete(&target, file)? {
            VerifyResult::Ok => {
                done += 1;
                touch_file_progress(progress, done, total, &file.path, "ok");
            }
            other => {
                let _ = std::fs::remove_file(&target);
                let _ = std::fs::remove_file(part_path(&target));
                let _ = std::fs::remove_file(ok_path(&target));
                return Err(format!(
                    "{}: integrity check failed after download ({other:?})",
                    file.path
                ));
            }
        }
    }
    Ok(())
}

#[derive(Debug)]
enum VerifyResult {
    Ok,
    Missing,
    Incomplete,
    Corrupt,
}

fn verify_complete(path: &Path, file: &RemoteFile) -> Result<VerifyResult, String> {
    let Ok(meta) = std::fs::metadata(path) else {
        return Ok(VerifyResult::Missing);
    };
    if !meta.is_file() || meta.len() == 0 {
        return Ok(VerifyResult::Missing);
    }
    if let Some(n) = file.size {
        if meta.len() != n {
            return Ok(if meta.len() < n {
                VerifyResult::Incomplete
            } else {
                VerifyResult::Corrupt
            });
        }
    }
    if let Some(expected) = &file.sha256 {
        let ok_path = ok_path(path);
        if let Ok(cached) = std::fs::read_to_string(&ok_path) {
            if cached.trim().eq_ignore_ascii_case(expected) {
                return Ok(VerifyResult::Ok);
            }
        }
        let got = sha256_file(path)?;
        if got != *expected {
            let _ = std::fs::remove_file(ok_path);
            return Ok(VerifyResult::Corrupt);
        }
        let _ = std::fs::write(ok_path, &got);
    } else if let Some(n) = file.size {
        // Size-only trust marker when remote has no hash.
        let _ = std::fs::write(ok_path(path), format!("size:{n}"));
    }
    Ok(VerifyResult::Ok)
}

fn ok_path(target: &Path) -> PathBuf {
    let name = target
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    target
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!(".{name}.h3ok"))
}

fn file_urls(source: DownloadSource, repo: &str, path: &str) -> Vec<String> {
    let encoded = path
        .split('/')
        .map(percent_encode)
        .collect::<Vec<_>>()
        .join("/");
    match source {
        DownloadSource::Huggingface | DownloadSource::HfMirror => {
            let endpoint = source
                .hf_endpoint()
                .unwrap_or("https://huggingface.co");
            vec![format!("{endpoint}/{repo}/resolve/main/{encoded}")]
        }
        DownloadSource::Modelscope => vec![
            format!(
                "https://www.modelscope.cn/api/v1/models/{repo}/repo?Revision=master&FilePath={}",
                percent_encode(path)
            ),
            format!("https://www.modelscope.cn/models/{repo}/resolve/master/{encoded}"),
        ],
    }
}

fn part_path(target: &Path) -> PathBuf {
    let name = target
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    target
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!(".{name}.h3part"))
}

fn download_one(
    client: &reqwest::blocking::Client,
    url: &str,
    target: &Path,
    file: &RemoteFile,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let tmp = part_path(target);
    let mut existing = 0u64;
    if tmp.is_file() {
        existing = std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);
        if let Some(n) = file.size {
            if existing > n {
                let _ = std::fs::remove_file(&tmp);
                existing = 0;
            } else if existing == n {
                // Finished part awaiting rename/verify.
                finalize_part(&tmp, target, file)?;
                return Ok(());
            }
        }
    }

    let mut req = client.get(url);
    if url.contains("huggingface.co") || url.contains("hf-mirror.com") {
        if let Ok(token) = std::env::var("HF_TOKEN") {
            if !token.is_empty() {
                req = req.bearer_auth(token);
            }
        }
    }
    if existing > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={existing}-"));
    }

    let mut resp = req
        .send()
        .map_err(|e| format!("GET {}: {e}", target.display()))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!(
            "{}: HTTP {status} ({url})",
            target
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("file")
        ));
    }

    // 206 Partial Content, or 200 + Content-Range (ModelScope style).
    let range_start = resp
        .headers()
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_content_range_start);
    let resumed = existing > 0
        && (status.as_u16() == 206 || range_start == Some(existing));
    if existing > 0 && !resumed {
        // Server ignored / reset Range — restart the part file.
        existing = 0;
        let _ = std::fs::remove_file(&tmp);
    }

    let mut out = if resumed {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&tmp)
            .map_err(|e| e.to_string())?
    } else {
        File::create(&tmp).map_err(|e| e.to_string())?
    };

    let mut buf = [0u8; CHUNK];
    let mut written = existing;
    loop {
        if cancel.load(Ordering::SeqCst) {
            // Keep .h3part for resume.
            drop(out);
            return Err("cancelled".into());
        }
        let n = resp.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        written += n as u64;
    }
    out.flush().map_err(|e| e.to_string())?;
    drop(out);

    if let Some(n) = file.size {
        if written != n {
            // Incomplete (network cut) — keep part for resume.
            return Err(format!(
                "{}: incomplete (got {written}, expected {n})",
                target.display()
            ));
        }
    }

    finalize_part(&tmp, target, file)
}

fn finalize_part(tmp: &Path, target: &Path, file: &RemoteFile) -> Result<(), String> {
    if let Some(n) = file.size {
        let len = std::fs::metadata(tmp).map(|m| m.len()).map_err(|e| e.to_string())?;
        if len != n {
            return Err(format!(
                "{}: size mismatch before verify (got {len}, expected {n})",
                target.display()
            ));
        }
    }
    if let Some(expected) = &file.sha256 {
        let got = sha256_file(tmp)?;
        if got != *expected {
            let _ = std::fs::remove_file(tmp);
            return Err(format!(
                "{}: sha256 mismatch (got {got}, expected {expected})",
                target.display()
            ));
        }
        let _ = std::fs::write(ok_path(target), &got);
    } else if let Some(n) = file.size {
        let _ = std::fs::write(ok_path(target), format!("size:{n}"));
    }
    std::fs::rename(tmp, target).map_err(|e| e.to_string())?;
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; CHUNK];
    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn parse_content_range_start(header: &str) -> Option<u64> {
    // "bytes 4000-17131/17132" or "bytes 4000-*"
    let rest = header.trim().strip_prefix("bytes")?;
    let rest = rest.trim_start();
    let start = rest.split('-').next()?.trim();
    start.parse().ok()
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

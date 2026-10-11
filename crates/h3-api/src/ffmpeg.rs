use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct FfmpegStatus {
    pub ffmpeg: bool,
    pub ffprobe: bool,
    pub ffmpeg_path: Option<String>,
    pub ffprobe_path: Option<String>,
    pub source: Option<&'static str>,
}

/// Resolve FFmpeg/FFprobe and export `H3_FFMPEG` / `H3_FFPROBE` for libh3.
/// Prefer explicit env → bundled next to the binary / vendor → PATH.
pub fn ensure_env() -> FfmpegStatus {
    let ffmpeg = resolve_tool("ffmpeg", "H3_FFMPEG");
    let ffprobe = resolve_tool("ffprobe", "H3_FFPROBE");

    if let Some((path, _)) = &ffmpeg {
        env::set_var("H3_FFMPEG", path);
        prepend_path_dir(path);
    }
    if let Some((path, _)) = &ffprobe {
        env::set_var("H3_FFPROBE", path);
        prepend_path_dir(path);
    }

    let source = ffmpeg
        .as_ref()
        .map(|(_, s)| *s)
        .or_else(|| ffprobe.as_ref().map(|(_, s)| *s));

    FfmpegStatus {
        ffmpeg: ffmpeg.is_some(),
        ffprobe: ffprobe.is_some(),
        ffmpeg_path: ffmpeg.map(|(p, _)| p.to_string_lossy().into_owned()),
        ffprobe_path: ffprobe.map(|(p, _)| p.to_string_lossy().into_owned()),
        source,
    }
}

pub fn status() -> FfmpegStatus {
    let ffmpeg = current_or_resolve("ffmpeg", "H3_FFMPEG");
    let ffprobe = current_or_resolve("ffprobe", "H3_FFPROBE");
    let source = ffmpeg
        .as_ref()
        .and_then(|p| classify(p))
        .or_else(|| ffprobe.as_ref().and_then(|p| classify(p)));
    FfmpegStatus {
        ffmpeg: ffmpeg.as_ref().map(|p| p.is_file()).unwrap_or(false),
        ffprobe: ffprobe.as_ref().map(|p| p.is_file()).unwrap_or(false),
        ffmpeg_path: ffmpeg.map(|p| p.to_string_lossy().into_owned()),
        ffprobe_path: ffprobe.map(|p| p.to_string_lossy().into_owned()),
        source,
    }
}

fn current_or_resolve(name: &str, env_key: &str) -> Option<PathBuf> {
    if let Ok(v) = env::var(env_key) {
        let p = PathBuf::from(v);
        if p.is_file() {
            return Some(p);
        }
    }
    resolve_tool(name, env_key).map(|(p, _)| p)
}

fn resolve_tool(name: &str, env_key: &str) -> Option<(PathBuf, &'static str)> {
    if let Ok(v) = env::var(env_key) {
        let p = PathBuf::from(&v);
        if p.is_file() {
            return Some((p, "env"));
        }
    }

    for dir in candidate_dirs() {
        let p = dir.join(name);
        if p.is_file() {
            return Some((p, "bundled"));
        }
    }

    which(name).map(|p| (p, "path"))
}

fn candidate_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Ok(dir) = env::var("H3_FFMPEG_DIR") {
        dirs.push(PathBuf::from(dir));
    }

    if let Ok(exe) = env::current_exe() {
        if let Some(parent) = exe.parent() {
            dirs.push(parent.to_path_buf());
            dirs.push(parent.join("resources"));
            dirs.push(parent.join("bin"));
        }
    }

    // Repo checkout (compile-time path): vendor/ffmpeg/<platform> + Tauri resources.
    {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        push_vendor(&mut dirs, &root);
        dirs.push(root.join("desktop/src-tauri/resources"));
    }
    if let Ok(cwd) = env::current_dir() {
        push_vendor(&mut dirs, &cwd);
        dirs.push(cwd.join("desktop/src-tauri/resources"));
        dirs.push(cwd.join("resources"));
    }

    dirs
}

fn push_vendor(dirs: &mut Vec<PathBuf>, root: &Path) {
    let platform = vendor_platform();
    dirs.push(root.join("vendor/ffmpeg").join(platform));
}

fn vendor_platform() -> &'static str {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "macos-arm64"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "macos-amd64"
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "linux-arm64"
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "linux-amd64"
    }
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64"),
    )))]
    {
        "unknown"
    }
}

fn which(bin: &str) -> Option<PathBuf> {
    let output = Command::new("which")
        .arg(bin)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        return None;
    }
    let p = PathBuf::from(path);
    p.is_file().then_some(p)
}

fn prepend_path_dir(bin: &Path) {
    let Some(dir) = bin.parent() else {
        return;
    };
    let dir = dir.to_string_lossy();
    match env::var("PATH") {
        Ok(path) if path.split(':').any(|p| p == dir) => {}
        Ok(path) => env::set_var("PATH", format!("{dir}:{path}")),
        Err(_) => env::set_var("PATH", dir.as_ref()),
    }
}

fn classify(path: &Path) -> Option<&'static str> {
    let s = path.to_string_lossy();
    if s.contains("/vendor/ffmpeg/") || s.contains("/resources/") {
        Some("bundled")
    } else if env::var_os("H3_FFMPEG").is_some() || env::var_os("H3_FFPROBE").is_some() {
        Some("env")
    } else {
        Some("path")
    }
}

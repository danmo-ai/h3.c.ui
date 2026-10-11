use std::fs::{self, OpenOptions};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Manager, RunEvent};
#[cfg(not(debug_assertions))]
use tauri::Url;

const API_PID_FILE: &str = "api.pid";
const API_PORT_FILE: &str = "api.port";

struct ApiProcess {
    child: Mutex<Option<Child>>,
    data_dir: Mutex<Option<PathBuf>>,
}

impl ApiProcess {
    fn new() -> Self {
        Self {
            child: Mutex::new(None),
            data_dir: Mutex::new(None),
        }
    }

    fn shutdown(&self) {
        if let Ok(mut guard) = self.child.lock() {
            if let Some(mut child) = guard.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        if let Ok(dir) = self.data_dir.lock() {
            if let Some(dir) = dir.as_ref() {
                let _ = fs::remove_file(dir.join(API_PID_FILE));
                let _ = fs::remove_file(dir.join(API_PORT_FILE));
            }
        }
    }
}

impl Drop for ApiProcess {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn data_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    let dir = home.join(".h3-ui");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn pick_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .ok()
        .and_then(|l| l.local_addr().ok())
        .map(|a| a.port())
        .unwrap_or(7810)
}

fn repo_root() -> PathBuf {
    // desktop/src-tauri -> repo
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("../.."))
}

fn find_h3_api(app: &AppHandle) -> Result<PathBuf, String> {
    let root = repo_root();
    // Dev: prefer freshly built crates binary over a stale Tauri resource copy.
    #[cfg(debug_assertions)]
    {
        let candidates = [
            root.join("crates/target/debug/h3-api"),
            root.join("desktop/src-tauri/resources/h3-api"),
        ];
        for c in &candidates {
            if c.is_file() {
                return Ok(c.clone());
            }
        }
    }
    if let Ok(res) = app.path().resource_dir() {
        let bundled = res.join("h3-api");
        if bundled.is_file() {
            return Ok(bundled);
        }
        let nested = res.join("resources").join("h3-api");
        if nested.is_file() {
            return Ok(nested);
        }
    }
    let candidates = [
        root.join("crates/target/release/h3-api"),
        root.join("crates/target/debug/h3-api"),
        root.join("desktop/src-tauri/resources/h3-api"),
    ];
    for c in candidates {
        if c.is_file() {
            return Ok(c);
        }
    }
    Err("h3-api binary not found; run `make api` or `make pack` first".into())
}

fn vendor_ffmpeg_dir(root: &Path) -> PathBuf {
    let triple = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "macos-arm64"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "macos-amd64"
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        "linux-arm64"
    } else {
        "linux-amd64"
    };
    root.join("vendor/ffmpeg").join(triple)
}

fn find_media_tool(app: &AppHandle, name: &str) -> Option<PathBuf> {
    if let Ok(res) = app.path().resource_dir() {
        for dir in [res.clone(), res.join("resources"), res.join("bin")] {
            let p = dir.join(name);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    let root = repo_root();
    for dir in [
        root.join("desktop/src-tauri/resources"),
        vendor_ffmpeg_dir(&root),
    ] {
        let p = dir.join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

fn wait_for_health(port: u16, child: &mut Child, log_path: &Path) -> Result<(), String> {
    let url = format!("http://127.0.0.1:{port}/api/system/health");
    for _ in 0..120 {
        if let Ok(Some(status)) = child.try_wait() {
            let log = fs::read_to_string(log_path).unwrap_or_default();
            return Err(format!(
                "h3-api exited ({status}) before health.\n{log}"
            ));
        }
        if let Ok(resp) = ureq::get(&url).call() {
            if (200..300).contains(&resp.status()) {
                return Ok(());
            }
        }
        thread::sleep(Duration::from_millis(250));
    }
    Err(format!("Timed out waiting for {url}"))
}

fn spawn_api(app: &AppHandle, port: u16) -> Result<(), String> {
    let data = data_dir();
    let log_path = data.join("api.log");
    let api_bin = find_h3_api(app)?;
    let root = repo_root();
    let static_dir = root.join("out/frontend/dist");

    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| e.to_string())?;
    let log_err = log_file.try_clone().map_err(|e| e.to_string())?;

    let mut cmd = Command::new(&api_bin);
    cmd.args([
        "--host",
        "127.0.0.1",
        "--port",
        &port.to_string(),
        "--home",
        &data.to_string_lossy(),
    ]);
    if static_dir.is_dir() {
        cmd.args(["--static-dir", &static_dir.to_string_lossy()]);
    }
    // Ensure libh3.dylib is discoverable via rpath / DYLD
    let mut dylib_dirs = vec![root.display().to_string()];
    if let Ok(res) = app.path().resource_dir() {
        dylib_dirs.push(res.display().to_string());
        dylib_dirs.push(res.join("resources").display().to_string());
    }
    if let Some(parent) = api_bin.parent() {
        dylib_dirs.push(parent.display().to_string());
    }
    let joined = dylib_dirs.join(":");
    cmd.env("DYLD_LIBRARY_PATH", &joined);
    cmd.env("DYLD_FALLBACK_LIBRARY_PATH", &joined);

    // Prefer bundled static FFmpeg/FFprobe (libh3 reads H3_FFMPEG / H3_FFPROBE).
    let mut path_prefix = Vec::new();
    if let Some(ffmpeg) = find_media_tool(app, "ffmpeg") {
        cmd.env("H3_FFMPEG", &ffmpeg);
        if let Some(dir) = ffmpeg.parent() {
            path_prefix.push(dir.display().to_string());
        }
    }
    if let Some(ffprobe) = find_media_tool(app, "ffprobe") {
        cmd.env("H3_FFPROBE", &ffprobe);
        if let Some(dir) = ffprobe.parent() {
            let s = dir.display().to_string();
            if !path_prefix.iter().any(|p| p == &s) {
                path_prefix.push(s);
            }
        }
    }
    if !path_prefix.is_empty() {
        let prefix = path_prefix.join(":");
        match std::env::var("PATH") {
            Ok(path) => cmd.env("PATH", format!("{prefix}:{path}")),
            Err(_) => cmd.env("PATH", prefix),
        };
    }

    cmd.stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_err));

    let mut child = cmd.spawn().map_err(|e| format!("spawn h3-api: {e}"))?;
    wait_for_health(port, &mut child, &log_path)?;

    let _ = fs::write(data.join(API_PID_FILE), child.id().to_string());
    let _ = fs::write(data.join(API_PORT_FILE), port.to_string());

    let state = app.state::<ApiProcess>();
    *state.child.lock().map_err(|e| e.to_string())? = Some(child);
    *state.data_dir.lock().map_err(|e| e.to_string())? = Some(data);
    Ok(())
}

#[cfg(target_os = "macos")]
fn apply_macos_shell(app: &AppHandle) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial};
    if let Err(err) =
        apply_vibrancy(&win, NSVisualEffectMaterial::UnderWindowBackground, None, None)
    {
        eprintln!("[h3-desktop] macOS window vibrancy failed: {err}");
    }
    if let Err(err) = win.set_theme(Some(tauri::Theme::Dark)) {
        eprintln!("[h3-desktop] macOS window theme failed: {err}");
    }
    let _ = win.eval(r#"document.documentElement.classList.add('dq-tauri-macos');"#);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(ApiProcess::new())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            apply_macos_shell(app.handle());

            let handle = app.handle().clone();
            // Dev UI (Vite :5810) proxies /api → :7810. Keep that fixed in debug
            // so the webview does not get Bad Gateway from a random sidecar port.
            let port = std::env::var("H3_API_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(|| {
                    if cfg!(debug_assertions) {
                        7810
                    } else {
                        pick_port()
                    }
                });

            thread::spawn(move || {
                if let Err(e) = spawn_api(&handle, port) {
                    eprintln!("[h3-desktop] API start failed: {e}");
                    return;
                }
                eprintln!("[h3-desktop] API ready on :{port}");

                #[cfg(not(debug_assertions))]
                {
                    if let Some(win) = handle.get_webview_window("main") {
                        let url =
                            Url::parse(&format!("http://127.0.0.1:{port}/")).expect("url");
                        let _ = win.navigate(url);
                    }
                }
                #[cfg(debug_assertions)]
                {
                    let _ = port;
                    eprintln!("[h3-desktop] API ready; UI via Vite :5810");
                }
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if matches!(event, RunEvent::Exit | RunEvent::ExitRequested { .. }) {
                if let Some(api) = app_handle.try_state::<ApiProcess>() {
                    api.shutdown();
                }
            }
        });
}

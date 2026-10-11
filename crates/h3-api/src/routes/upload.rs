use std::path::{Path, PathBuf};

use axum::extract::{DefaultBodyLimit, Multipart, Path as AxumPath, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::state::AppState;

const MAX_UPLOAD_BYTES: usize = 200 * 1024 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(create))
        .route("/{name}/file", get(file))
        .layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES))
}

fn uploads_dir(state: &AppState) -> PathBuf {
    state.home.join("uploads")
}

pub async fn create(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<Value>, (StatusCode, String)> {
    let dir = uploads_dir(&state);
    std::fs::create_dir_all(&dir).map_err(internal)?;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("multipart error: {e}"),
        )
    })? {
        if field.name() != Some("file") {
            continue;
        }

        let original = field
            .file_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "upload.bin".into());
        let content_type = field
            .content_type()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "application/octet-stream".into());

        let ext = extension_of(&original);
        validate_extension(&ext)?;

        let bytes = field.bytes().await.map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                format!("read upload failed: {e}"),
            )
        })?;
        if bytes.is_empty() {
            return Err((StatusCode::BAD_REQUEST, "empty file".into()));
        }
        if bytes.len() > MAX_UPLOAD_BYTES {
            return Err((StatusCode::PAYLOAD_TOO_LARGE, "file too large".into()));
        }

        let safe_stem = sanitize_stem(&original);
        let name = format!("{}_{}.{}", Uuid::new_v4().simple(), safe_stem, ext);
        let path = dir.join(&name);
        std::fs::write(&path, &bytes).map_err(internal)?;

        return Ok(Json(json!({
            "name": name,
            "path": path.to_string_lossy(),
            "bytes": bytes.len(),
            "content_type": content_type,
            "original_name": original,
            "kind": kind_for_ext(&ext),
        })));
    }

    Err((StatusCode::BAD_REQUEST, "missing file field".into()))
}

pub async fn file(
    State(state): State<AppState>,
    AxumPath(name): AxumPath<String>,
) -> Result<Response, (StatusCode, String)> {
    let safe = sanitize_stored_name(&name)?;
    let path = uploads_dir(&state).join(&safe);
    if !path.is_file() {
        return Err((StatusCode::NOT_FOUND, "upload not found".into()));
    }
    let bytes = std::fs::read(&path).map_err(internal)?;
    let ct = content_type_for_path(&path);
    let mut res = bytes.into_response();
    if let Ok(v) = header::HeaderValue::from_str(ct) {
        res.headers_mut().insert(header::CONTENT_TYPE, v);
    }
    if let Ok(v) = header::HeaderValue::from_str(&format!("inline; filename=\"{safe}\"")) {
        res.headers_mut().insert(header::CONTENT_DISPOSITION, v);
    }
    Ok(res)
}

fn internal(e: impl ToString) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

fn extension_of(name: &str) -> String {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin")
        .to_ascii_lowercase()
}

fn validate_extension(ext: &str) -> Result<(), (StatusCode, String)> {
    const ALLOWED: &[&str] = &[
        "png", "jpg", "jpeg", "webp", "bmp", "gif", // image
        "mp4", "mov", "webm", "mkv", "m4v", // video
        "wav", "mp3", "m4a", "aac", "flac", "ogg", // audio
    ];
    if ALLOWED.contains(&ext) {
        Ok(())
    } else {
        Err((
            StatusCode::BAD_REQUEST,
            format!("unsupported file type .{ext}"),
        ))
    }
}

fn kind_for_ext(ext: &str) -> &'static str {
    match ext {
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif" => "image",
        "mp4" | "mov" | "webm" | "mkv" | "m4v" => "video",
        "wav" | "mp3" | "m4a" | "aac" | "flac" | "ogg" => "audio",
        _ => "file",
    }
}

fn sanitize_stem(original: &str) -> String {
    let stem = Path::new(original)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let mut out = String::new();
    for ch in stem.chars().take(48) {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else if ch.is_whitespace() {
            out.push('_');
        }
    }
    if out.is_empty() {
        "file".into()
    } else {
        out
    }
}

fn sanitize_stored_name(name: &str) -> Result<String, (StatusCode, String)> {
    if name.is_empty()
        || name.contains("..")
        || name.contains('/')
        || name.contains('\\')
        || name.starts_with('.')
    {
        return Err((StatusCode::BAD_REQUEST, "invalid name".into()));
    }
    let ext = extension_of(name);
    validate_extension(&ext)?;
    Ok(name.to_string())
}

fn content_type_for_path(path: &Path) -> &'static str {
    match extension_of(&path.to_string_lossy()).as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "gif" => "image/gif",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "flac" => "audio/flac",
        "ogg" => "audio/ogg",
        _ => "application/octet-stream",
    }
}

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};

use crate::state::AppState;

pub async fn list(State(state): State<AppState>) -> Json<Value> {
    let dir = state.config.lock().output_dir.clone();
    let mut items = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("mp4") {
                continue;
            }
            let meta = e.metadata().ok();
            items.push(json!({
                "name": path.file_name().and_then(|n| n.to_str()).unwrap_or_default(),
                "path": path.to_string_lossy(),
                "bytes": meta.as_ref().map(|m| m.len()).unwrap_or(0),
                "modified": meta.and_then(|m| m.modified().ok()).and_then(|t| {
                    t.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs())
                }),
            }));
        }
    }
    items.sort_by(|a, b| {
        let am = a.get("modified").and_then(|v| v.as_u64()).unwrap_or(0);
        let bm = b.get("modified").and_then(|v| v.as_u64()).unwrap_or(0);
        bm.cmp(&am)
    });
    Json(json!({ "outputs": items }))
}

pub async fn remove(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let safe = sanitize_name(&name)?;
    let path = state.config.lock().output_dir.join(&safe);
    if !path.is_file() {
        return Err((StatusCode::NOT_FOUND, "output not found".into()));
    }
    std::fs::remove_file(&path).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn file(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Response, (StatusCode, String)> {
    let safe = sanitize_name(&name)?;
    let path = state.config.lock().output_dir.join(&safe);
    if !path.is_file() {
        return Err((StatusCode::NOT_FOUND, "output not found".into()));
    }
    let bytes = std::fs::read(&path).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let mut res = bytes.into_response();
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("video/mp4"),
    );
    if let Ok(v) = header::HeaderValue::from_str(&format!("inline; filename=\"{safe}\"")) {
        res.headers_mut().insert(header::CONTENT_DISPOSITION, v);
    }
    Ok(res)
}

fn sanitize_name(name: &str) -> Result<String, (StatusCode, String)> {
    if name.contains("..") || name.contains('/') || name.contains('\\') {
        return Err((StatusCode::BAD_REQUEST, "invalid name".into()));
    }
    if !name.ends_with(".mp4") {
        return Err((StatusCode::BAD_REQUEST, "only .mp4 supported".into()));
    }
    Ok(name.to_string())
}

use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::engine::model_status_from_dir;
use crate::state::AppState;

pub async fn status(State(state): State<AppState>) -> Json<Value> {
    let cfg = state.config.lock().clone();
    let fs = model_status_from_dir(&cfg.model_dir);
    let mut loaded = false;
    let mut model = None;
    if fs.fl2va_ready {
        if state
            .engine
            .ensure_loaded(&cfg.model_dir, cfg.cache_enabled)
            .is_ok()
        {
            loaded = true;
            model = state.engine.model_info().ok();
        }
    }
    let sources: Vec<Value> = crate::config::DownloadSource::all()
        .iter()
        .copied()
        .map(|s| {
            json!({
                "id": s.as_str(),
                "repo": cfg.repo_for_source(s),
                "endpoint": s.hf_endpoint(),
            })
        })
        .collect();
    Json(json!({
        "model_dir": fs.model_dir,
        "fl2va_ready": fs.fl2va_ready,
        "ref2va_ready": fs.ref2va_ready,
        "missing_paths": fs.missing_paths,
        "disk_bytes": fs.disk_bytes,
        "loaded": loaded,
        "model": model,
        "hf_repo": cfg.hf_repo,
        "modelscope_repo": cfg.modelscope_repo,
        "download_source": cfg.download_source.as_str(),
        "sources": sources,
    }))
}

pub async fn load(
    State(state): State<AppState>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let cfg = state.config.lock().clone();
    state
        .engine
        .ensure_loaded(&cfg.model_dir, cfg.cache_enabled)
        .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e.to_string()))?;
    let model = state
        .engine
        .model_info()
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let device = state
        .engine
        .device_info()
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({ "ok": true, "model": model, "device": device })))
}

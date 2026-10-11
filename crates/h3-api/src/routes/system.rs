use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::ffmpeg as media;
use crate::state::AppState;

pub async fn health() -> Json<Value> {
    Json(json!({
        "ok": true,
        "service": "h3-api",
        "engine_version": crate::engine::Engine::version(),
    }))
}

pub async fn device(State(state): State<AppState>) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let cfg = state.config.lock().clone();
    if let Err(e) = state.engine.ensure_loaded(&cfg.model_dir, cfg.cache_enabled) {
        return Err((axum::http::StatusCode::BAD_REQUEST, e.to_string()));
    }
    let device = state
        .engine
        .device_info()
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let model = state
        .engine
        .model_info()
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({ "device": device, "model": model })))
}

pub async fn ffmpeg() -> Json<Value> {
    let s = media::status();
    Json(json!({
        "ffmpeg": s.ffmpeg,
        "ffprobe": s.ffprobe,
        "ffmpeg_path": s.ffmpeg_path,
        "ffprobe_path": s.ffprobe_path,
        "source": s.source,
    }))
}

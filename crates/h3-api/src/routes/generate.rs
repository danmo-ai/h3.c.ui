use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::engine::GenerateRequest;
use crate::state::AppState;

pub async fn start(
    State(state): State<AppState>,
    Json(req): Json<GenerateRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, String)> {
    if req.prompt.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "prompt is required".into()));
    }
    if !req.references.is_empty()
        && (req.first_frame.is_some() || req.last_frame.is_some())
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Ref2VA references are mutually exclusive with first/last frame".into(),
        ));
    }
    let cfg = state.config.lock().clone();
    state
        .tasks
        .update_paths(cfg.model_dir, cfg.output_dir, cfg.cache_enabled);
    let task_id = state.tasks.submit(req);
    Ok((StatusCode::ACCEPTED, Json(json!({ "task_id": task_id }))))
}

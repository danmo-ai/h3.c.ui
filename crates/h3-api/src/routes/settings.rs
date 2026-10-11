use axum::extract::State;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::state::AppState;

pub async fn get(State(state): State<AppState>) -> Json<Value> {
    let cfg = state.config.lock().clone();
    Json(json!({
        "home": state.home,
        "model_dir": cfg.model_dir,
        "output_dir": cfg.output_dir,
        "cache_enabled": cfg.cache_enabled,
        "hf_repo": cfg.hf_repo,
        "modelscope_repo": cfg.modelscope_repo,
        "download_source": cfg.download_source.as_str(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct SettingsUpdate {
    pub model_dir: Option<String>,
    pub output_dir: Option<String>,
    pub cache_enabled: Option<bool>,
    pub hf_repo: Option<String>,
    pub modelscope_repo: Option<String>,
    pub download_source: Option<String>,
}

pub async fn put(
    State(state): State<AppState>,
    Json(body): Json<SettingsUpdate>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let mut cfg = state.config.lock().clone();
    if let Some(v) = body.model_dir {
        cfg.model_dir = v.into();
    }
    if let Some(v) = body.output_dir {
        cfg.output_dir = v.into();
    }
    if let Some(v) = body.cache_enabled {
        cfg.cache_enabled = v;
    }
    if let Some(v) = body.hf_repo {
        cfg.hf_repo = v;
    }
    if let Some(v) = body.modelscope_repo {
        cfg.modelscope_repo = v;
    }
    if let Some(v) = body.download_source {
        cfg.download_source = crate::config::DownloadSource::parse(&v).ok_or_else(|| {
            (
                axum::http::StatusCode::BAD_REQUEST,
                format!("unknown download_source: {v}"),
            )
        })?;
    }
    cfg.save(&state.home)
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    state.tasks.update_paths(
        cfg.model_dir.clone(),
        cfg.output_dir.clone(),
        cfg.cache_enabled,
    );
    *state.config.lock() = cfg.clone();
    Ok(Json(json!({ "ok": true, "settings": {
        "model_dir": cfg.model_dir,
        "output_dir": cfg.output_dir,
        "cache_enabled": cfg.cache_enabled,
        "hf_repo": cfg.hf_repo,
        "modelscope_repo": cfg.modelscope_repo,
        "download_source": cfg.download_source.as_str(),
    }})))
}

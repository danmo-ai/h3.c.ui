use std::convert::Infallible;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use futures_util::stream;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt as _;

use crate::config::DownloadSource;
use crate::download::DownloadPackage;
use crate::state::AppState;

#[derive(Debug, Default, Deserialize)]
pub struct StartDownloadBody {
    pub source: Option<String>,
    /// `fl2va` (text / first-last) · `ref2va` (references) · `all`
    pub package: Option<String>,
}

pub async fn start(
    State(state): State<AppState>,
    Json(body): Json<StartDownloadBody>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let mut cfg = state.config.lock().clone();
    let source = body
        .source
        .as_deref()
        .and_then(DownloadSource::parse)
        .unwrap_or(cfg.download_source);
    let package = body
        .package
        .as_deref()
        .and_then(DownloadPackage::parse)
        .unwrap_or(DownloadPackage::Fl2va);

    cfg.download_source = source;
    let repo = cfg.repo_for_source(source).to_string();
    cfg.save(&state.home)
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    *state.config.lock() = cfg.clone();

    let (task_id, _, _) = state
        .downloads
        .start(source, repo.clone(), cfg.model_dir.clone(), package)
        .map_err(|e| (axum::http::StatusCode::CONFLICT, e))?;
    Ok(Json(json!({
        "task_id": task_id,
        "source": source.as_str(),
        "repo": repo,
        "package": package.as_str(),
    })))
}

pub async fn current(State(state): State<AppState>) -> Json<Value> {
    match state.downloads.current_progress() {
        Some(p) => Json(serde_json::to_value(p).unwrap_or_else(|_| json!({}))),
        None => Json(json!({ "task_id": null, "status": "idle" })),
    }
}

pub async fn cancel(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    if state.downloads.cancel(&task_id) {
        Ok(Json(json!({ "ok": true })))
    } else {
        Err((
            axum::http::StatusCode::NOT_FOUND,
            "download task not found".into(),
        ))
    }
}

pub async fn stream(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, (axum::http::StatusCode, String)>
{
    if state.downloads.get_progress(&task_id).is_none() {
        return Err((
            axum::http::StatusCode::NOT_FOUND,
            "download task not found".into(),
        ));
    }

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(16);
    let downloads = state.downloads.clone();
    tokio::spawn(async move {
        loop {
            let prog = downloads.get_progress(&task_id);
            let Some(p) = prog else {
                break;
            };
            let data = serde_json::to_string(&p).unwrap_or_else(|_| "{}".into());
            if tx.send(Ok(Event::default().event("progress").data(data))).await.is_err()
            {
                break;
            }
            use crate::download::DownloadStatus;
            if matches!(
                p.status,
                DownloadStatus::Completed | DownloadStatus::Failed | DownloadStatus::Cancelled
            ) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
    });

    let stream = ReceiverStream::new(rx).chain(stream::empty());
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use futures_util::stream;
use serde_json::Value;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt as _;

use crate::state::AppState;
use crate::tasks::TaskStatus;

pub async fn list(State(state): State<AppState>) -> Json<Value> {
    Json(serde_json::to_value(state.tasks.list_recent()).unwrap_or_default())
}

pub async fn list_failed(State(state): State<AppState>) -> Json<Value> {
    Json(serde_json::json!({
        "tasks": state.tasks.list_failed(),
    }))
}

pub async fn get(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let snap = state
        .tasks
        .get(&task_id)
        .ok_or((axum::http::StatusCode::NOT_FOUND, "task not found".into()))?;
    Ok(Json(serde_json::to_value(snap).unwrap_or_default()))
}

pub async fn cancel(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    if state.tasks.cancel(&task_id) {
        Ok(Json(serde_json::json!({ "ok": true })))
    } else {
        Err((axum::http::StatusCode::NOT_FOUND, "task not found".into()))
    }
}

pub async fn stream(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, (axum::http::StatusCode, String)>
{
    let (initial, mut rx) = state.tasks.subscribe(&task_id).ok_or((
        axum::http::StatusCode::NOT_FOUND,
        "task not found".into(),
    ))?;

    let (tx, rx_out) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(32);

    tokio::spawn(async move {
        let data = serde_json::to_string(&initial).unwrap_or_else(|_| "{}".into());
        if tx
            .send(Ok(Event::default().event("progress").data(data)))
            .await
            .is_err()
        {
            return;
        }
        if matches!(
            initial.status,
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
        ) {
            return;
        }

        loop {
            match tokio::time::timeout(Duration::from_secs(30), rx.recv()).await {
                Ok(Ok(ev)) => {
                    let done = matches!(
                        ev.snapshot.status,
                        TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
                    );
                    let data = serde_json::to_string(&ev.snapshot).unwrap_or_else(|_| "{}".into());
                    if tx
                        .send(Ok(Event::default().event("progress").data(data)))
                        .await
                        .is_err()
                    {
                        break;
                    }
                    if done {
                        break;
                    }
                }
                Ok(Err(_)) => break,
                Err(_) => {
                    // keepalive tick via SSE KeepAlive; also re-send current if needed
                }
            }
        }
    });

    let stream = ReceiverStream::new(rx_out).chain(stream::empty());
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

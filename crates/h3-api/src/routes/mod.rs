mod download;
mod generate;
mod model;
mod outputs;
mod settings;
mod system;
mod tasks;
mod upload;

use axum::routing::{delete, get, post};
use axum::Router;

use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/system/health", get(system::health))
        .route("/api/system/device", get(system::device))
        .route("/api/system/ffmpeg", get(system::ffmpeg))
        .route("/api/model/status", get(model::status))
        .route("/api/model/load", post(model::load))
        .route("/api/model/download", post(download::start))
        .route("/api/model/download/current", get(download::current))
        .route(
            "/api/model/download/{task_id}/stream",
            get(download::stream),
        )
        .route(
            "/api/model/download/{task_id}/cancel",
            post(download::cancel),
        )
        .route("/api/generate", post(generate::start))
        .route("/api/tasks", get(tasks::list))
        .route("/api/tasks/{task_id}", get(tasks::get))
        .route("/api/tasks/{task_id}/stream", get(tasks::stream))
        .route("/api/tasks/{task_id}/cancel", post(tasks::cancel))
        .route("/api/outputs", get(outputs::list))
        .route("/api/outputs/{name}", delete(outputs::remove))
        .route("/api/outputs/{name}/file", get(outputs::file))
        .route("/api/settings", get(settings::get).put(settings::put))
        .nest("/api/uploads", upload::router())
        .with_state(state)
}

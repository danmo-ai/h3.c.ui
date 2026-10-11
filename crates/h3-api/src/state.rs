use std::sync::Arc;

use parking_lot::Mutex;

use crate::config::AppConfig;
use crate::download::DownloadManager;
use crate::engine::Engine;
use crate::tasks::TaskManager;

#[derive(Clone)]
pub struct AppState {
    pub home: std::path::PathBuf,
    pub config: Arc<Mutex<AppConfig>>,
    pub engine: Arc<Engine>,
    pub downloads: Arc<DownloadManager>,
    pub tasks: Arc<TaskManager>,
}

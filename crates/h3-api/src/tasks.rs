//! Serial generation task queue with SSE-friendly progress.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use parking_lot::{Condvar, Mutex};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::engine::{Engine, GenerateRequest, GenerateResult, ProgressEvent};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSnapshot {
    pub task_id: String,
    pub status: TaskStatus,
    pub phase: String,
    pub completed: i32,
    pub total: i32,
    pub progress: f64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub preview_jpeg_base64: Option<String>,
    pub result: Option<GenerateResult>,
    pub error: Option<String>,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub output_name: Option<String>,
    #[serde(default)]
    pub width: i32,
    #[serde(default)]
    pub height: i32,
    #[serde(default)]
    pub frames: Option<i32>,
    #[serde(default)]
    pub seconds: Option<f64>,
    #[serde(default)]
    pub created_ms: u64,
    #[serde(default)]
    pub finished_ms: Option<u64>,
}

#[derive(Clone)]
pub struct TaskEvent {
    pub snapshot: TaskSnapshot,
}

pub struct TaskManager {
    engine: Arc<Engine>,
    output_dir: Mutex<PathBuf>,
    model_dir: Mutex<PathBuf>,
    cache_enabled: Mutex<bool>,
    history_path: PathBuf,
    tasks: Mutex<HashMap<String, TaskInner>>,
    queue: Mutex<Vec<String>>,
    wake: Condvar,
}

struct TaskInner {
    snapshot: TaskSnapshot,
    cancel: Arc<AtomicBool>,
    tx: broadcast::Sender<TaskEvent>,
    request: GenerateRequest,
}

impl TaskManager {
    pub fn new(
        engine: Arc<Engine>,
        model_dir: PathBuf,
        output_dir: PathBuf,
        cache_enabled: bool,
        history_path: PathBuf,
    ) -> Arc<Self> {
        let mgr = Arc::new(Self {
            engine,
            output_dir: Mutex::new(output_dir),
            model_dir: Mutex::new(model_dir),
            cache_enabled: Mutex::new(cache_enabled),
            history_path,
            tasks: Mutex::new(HashMap::new()),
            queue: Mutex::new(Vec::new()),
            wake: Condvar::new(),
        });
        mgr.clone().spawn_worker();
        mgr
    }

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    fn snapshot_from_request(task_id: String, request: &GenerateRequest) -> TaskSnapshot {
        let prompt = if request.prompt.chars().count() > 160 {
            let mut s: String = request.prompt.chars().take(160).collect();
            s.push('…');
            s
        } else {
            request.prompt.clone()
        };
        TaskSnapshot {
            task_id,
            status: TaskStatus::Queued,
            phase: "queued".into(),
            completed: 0,
            total: 0,
            progress: 0.0,
            preview_jpeg_base64: None,
            result: None,
            error: None,
            prompt,
            output_name: request.output_name.clone(),
            width: request.width,
            height: request.height,
            frames: request.frames,
            seconds: request.seconds,
            created_ms: Self::now_ms(),
            finished_ms: None,
        }
    }

    fn persist_terminal(&self, snap: &TaskSnapshot) {
        if !matches!(
            snap.status,
            TaskStatus::Failed | TaskStatus::Completed | TaskStatus::Cancelled
        ) {
            return;
        }
        let mut entry = snap.clone();
        entry.preview_jpeg_base64 = None;
        let mut history = self.load_history();
        history.retain(|h| h.task_id != entry.task_id);
        history.insert(0, entry);
        history.truncate(100);
        if let Some(parent) = self.history_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(bytes) = serde_json::to_vec_pretty(&history) {
            let _ = std::fs::write(&self.history_path, bytes);
        }
    }

    fn load_history(&self) -> Vec<TaskSnapshot> {
        let Ok(bytes) = std::fs::read(&self.history_path) else {
            return Vec::new();
        };
        serde_json::from_slice(&bytes).unwrap_or_default()
    }

    pub fn update_paths(&self, model_dir: PathBuf, output_dir: PathBuf, cache_enabled: bool) {
        *self.model_dir.lock() = model_dir;
        *self.output_dir.lock() = output_dir;
        *self.cache_enabled.lock() = cache_enabled;
    }

    pub fn submit(&self, request: GenerateRequest) -> String {
        let task_id = Uuid::new_v4().to_string();
        let (tx, _) = broadcast::channel(64);
        let cancel = Arc::new(AtomicBool::new(false));
        let snapshot = Self::snapshot_from_request(task_id.clone(), &request);
        {
            let mut tasks = self.tasks.lock();
            tasks.insert(
                task_id.clone(),
                TaskInner {
                    snapshot,
                    cancel,
                    tx,
                    request,
                },
            );
            self.queue.lock().push(task_id.clone());
        }
        self.wake.notify_one();
        task_id
    }

    pub fn get(&self, task_id: &str) -> Option<TaskSnapshot> {
        self.tasks.lock().get(task_id).map(|t| t.snapshot.clone())
    }

    pub fn subscribe(
        &self,
        task_id: &str,
    ) -> Option<(TaskSnapshot, broadcast::Receiver<TaskEvent>)> {
        let tasks = self.tasks.lock();
        let t = tasks.get(task_id)?;
        Some((t.snapshot.clone(), t.tx.subscribe()))
    }

    pub fn cancel(&self, task_id: &str) -> bool {
        let mut tasks = self.tasks.lock();
        let Some(t) = tasks.get_mut(task_id) else {
            return false;
        };
        t.cancel.store(true, Ordering::SeqCst);
        if t.snapshot.status == TaskStatus::Queued {
            t.snapshot.status = TaskStatus::Cancelled;
            t.snapshot.phase = "cancelled".into();
            let _ = t.tx.send(TaskEvent {
                snapshot: t.snapshot.clone(),
            });
            self.queue.lock().retain(|id| id != task_id);
        }
        true
    }

    pub fn list_recent(&self) -> Vec<TaskSnapshot> {
        let mut v: Vec<_> = self
            .tasks
            .lock()
            .values()
            .map(|t| {
                let mut s = t.snapshot.clone();
                s.preview_jpeg_base64 = None;
                s
            })
            .collect();
        v.sort_by(|a, b| b.created_ms.cmp(&a.created_ms));
        v.truncate(50);
        v
    }

    /// Failed tasks from live memory + persisted history (newest first).
    pub fn list_failed(&self) -> Vec<TaskSnapshot> {
        let mut by_id: HashMap<String, TaskSnapshot> = HashMap::new();
        for h in self.load_history() {
            if h.status == TaskStatus::Failed {
                by_id.insert(h.task_id.clone(), h);
            }
        }
        for t in self.tasks.lock().values() {
            if t.snapshot.status == TaskStatus::Failed {
                let mut s = t.snapshot.clone();
                s.preview_jpeg_base64 = None;
                by_id.insert(s.task_id.clone(), s);
            }
        }
        let mut v: Vec<_> = by_id.into_values().collect();
        v.sort_by(|a, b| b.created_ms.cmp(&a.created_ms));
        v.truncate(50);
        v
    }

    fn mark_finished(&self, task_id: &str) {
        let mut tasks = self.tasks.lock();
        if let Some(t) = tasks.get_mut(task_id) {
            t.snapshot.finished_ms = Some(Self::now_ms());
            let snap = t.snapshot.clone();
            drop(tasks);
            self.persist_terminal(&snap);
        }
    }

    fn spawn_worker(self: Arc<Self>) {
        std::thread::Builder::new()
            .name("h3-generate-worker".into())
            .spawn(move || self.worker_loop())
            .expect("spawn generate worker");
    }

    fn worker_loop(self: Arc<Self>) {
        loop {
            let task_id = {
                let mut q = self.queue.lock();
                loop {
                    // Drop cancelled heads
                    while let Some(id) = q.first().cloned() {
                        let cancelled = self
                            .tasks
                            .lock()
                            .get(&id)
                            .map(|t| t.snapshot.status == TaskStatus::Cancelled)
                            .unwrap_or(true);
                        if cancelled {
                            q.remove(0);
                        } else {
                            break;
                        }
                    }
                    if let Some(id) = q.first().cloned() {
                        break id;
                    }
                    self.wake.wait_for(&mut q, Duration::from_secs(30));
                }
            };

            let (request, cancel, tx) = {
                let mut tasks = self.tasks.lock();
                let Some(t) = tasks.get_mut(&task_id) else {
                    self.queue.lock().retain(|id| id != &task_id);
                    continue;
                };
                t.snapshot.status = TaskStatus::Running;
                t.snapshot.phase = "starting".into();
                let _ = t.tx.send(TaskEvent {
                    snapshot: t.snapshot.clone(),
                });
                (t.request.clone(), t.cancel.clone(), t.tx.clone())
            };

            let model_dir = self.model_dir.lock().clone();
            let output_dir = self.output_dir.lock().clone();
            let cache_enabled = *self.cache_enabled.lock();

            if let Err(e) = self.engine.ensure_loaded(&model_dir, cache_enabled) {
                {
                    let mut tasks = self.tasks.lock();
                    if let Some(t) = tasks.get_mut(&task_id) {
                        t.snapshot.status = TaskStatus::Failed;
                        t.snapshot.error = Some(e.to_string());
                        t.snapshot.phase = "failed".into();
                        t.snapshot.finished_ms = Some(Self::now_ms());
                        let _ = t.tx.send(TaskEvent {
                            snapshot: t.snapshot.clone(),
                        });
                    }
                }
                self.mark_finished(&task_id);
                self.queue.lock().retain(|id| id != &task_id);
                continue;
            }

            let (prog_tx, prog_rx) = std::sync::mpsc::channel::<ProgressEvent>();
            let tx_progress = tx.clone();
            let task_id_prog = task_id.clone();
            let mgr_prog = self.clone();
            let progress_thread = std::thread::spawn(move || {
                while let Ok(ev) = prog_rx.recv() {
                    let mut tasks = mgr_prog.tasks.lock();
                    let Some(t) = tasks.get_mut(&task_id_prog) else {
                        break;
                    };
                    match ev {
                        ProgressEvent::Phase {
                            phase,
                            completed,
                            total,
                        } => {
                            t.snapshot.phase = phase;
                            t.snapshot.completed = completed;
                            t.snapshot.total = total;
                            if total > 0 {
                                t.snapshot.progress = (completed as f64 / total as f64) * 100.0;
                            }
                        }
                        ProgressEvent::Preview {
                            jpeg_base64,
                            denoise_step,
                            denoise_steps,
                            ..
                        } => {
                            t.snapshot.preview_jpeg_base64 = Some(jpeg_base64);
                            if denoise_steps > 0 {
                                t.snapshot.completed = denoise_step;
                                t.snapshot.total = denoise_steps;
                                t.snapshot.progress =
                                    (denoise_step as f64 / denoise_steps as f64) * 100.0;
                                t.snapshot.phase = "denoise".into();
                            }
                        }
                    }
                    let _ = tx_progress.send(TaskEvent {
                        snapshot: t.snapshot.clone(),
                    });
                }
            });

            let result =
                self.engine
                    .generate(&request, &output_dir, cancel.clone(), prog_tx);
            let _ = progress_thread.join();

            {
                let mut tasks = self.tasks.lock();
                if let Some(t) = tasks.get_mut(&task_id) {
                    match result {
                        Ok(r) => {
                            t.snapshot.status = TaskStatus::Completed;
                            t.snapshot.progress = 100.0;
                            t.snapshot.phase = "completed".into();
                            t.snapshot.result = Some(r);
                        }
                        Err(crate::engine::EngineError::Cancelled) => {
                            t.snapshot.status = TaskStatus::Cancelled;
                            t.snapshot.phase = "cancelled".into();
                        }
                        Err(e) => {
                            t.snapshot.status = TaskStatus::Failed;
                            t.snapshot.error = Some(e.to_string());
                            t.snapshot.phase = "failed".into();
                        }
                    }
                    t.snapshot.finished_ms = Some(Self::now_ms());
                    let _ = t.tx.send(TaskEvent {
                        snapshot: t.snapshot.clone(),
                    });
                }
            }
            self.mark_finished(&task_id);
            self.queue.lock().retain(|id| id != &task_id);
        }
    }
}

//! Safe wrapper around `h3-sys` / `libh3.dylib`.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;
use serde::Serialize;
use thiserror::Error;

use h3_sys::{
    h3_cache_clear, h3_cache_get_info, h3_cache_set_enabled, h3_ctx, h3_device, h3_free,
    h3_frame, h3_generate, h3_last_error, h3_load_dir, h3_model, h3_params, h3_reference,
    h3_reference_image_size_H3_REFERENCE_IMAGE_MATCH,
    h3_reference_image_size_H3_REFERENCE_IMAGE_MAX, h3_reference_kind_H3_REFERENCE_AUDIO,
    h3_reference_kind_H3_REFERENCE_IMAGE, h3_reference_kind_H3_REFERENCE_VIDEO,
    h3_reference_kind_H3_REFERENCE_VIDEO_AUDIO, h3_result_free, H3_DEFAULT_DIT_LAYERS,
    H3_DEFAULT_FRAMES, H3_DEFAULT_HEIGHT, H3_DEFAULT_STEPS, H3_DEFAULT_WIDTH, H3_VERSION,
};

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("{0}")]
    Message(String),
    #[error("model directory missing: {0}")]
    MissingModel(PathBuf),
    #[error("cancelled")]
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceInfo {
    pub name: String,
    pub architecture: String,
    pub physical_memory: u64,
    pub recommended_working_set: u64,
    pub max_buffer_length: u64,
    pub apple_gpu_family: i32,
    pub metal4: bool,
    pub unified_memory: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ComponentInfo {
    pub bytes: u64,
    pub tensor_bytes: u64,
    pub files: u64,
    pub tensors: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    pub text_encoder: ComponentInfo,
    pub fl2va_transformer: ComponentInfo,
    pub ref2va_transformer: ComponentInfo,
    pub video_vae: ComponentInfo,
    pub audio_vae: ComponentInfo,
}

#[derive(Debug, Clone, Serialize)]
pub struct CacheInfo {
    pub embedding_entries: u64,
    pub embedding_bytes: u64,
    pub prepared_dit: bool,
    pub video_decoder: bool,
}

#[derive(Debug, Clone)]
pub enum ProgressEvent {
    Phase {
        phase: String,
        completed: i32,
        total: i32,
    },
    Preview {
        jpeg_base64: String,
        denoise_step: i32,
        denoise_steps: i32,
        frame_index: i32,
        frame_count: i32,
        width: i32,
        height: i32,
    },
}

struct CallbackBridge {
    cancel: Arc<AtomicBool>,
    tx: std::sync::mpsc::Sender<ProgressEvent>,
}

extern "C" fn on_progress_cb(
    phase: *const c_char,
    completed: c_int,
    total: c_int,
    opaque: *mut c_void,
) -> c_int {
    if opaque.is_null() {
        return 0;
    }
    let bridge = unsafe { &*(opaque as *const CallbackBridge) };
    let phase = if phase.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(phase) }
            .to_string_lossy()
            .into_owned()
    };
    let _ = bridge.tx.send(ProgressEvent::Phase {
        phase,
        completed,
        total,
    });
    if bridge.cancel.load(Ordering::SeqCst) {
        1
    } else {
        0
    }
}

extern "C" fn on_frame_cb(frame: *const h3_frame, opaque: *mut c_void) -> c_int {
    if opaque.is_null() || frame.is_null() {
        return 0;
    }
    let bridge = unsafe { &*(opaque as *const CallbackBridge) };
    let f = unsafe { &*frame };
    if f.rgb.is_null() || f.width <= 0 || f.height <= 0 {
        return if bridge.cancel.load(Ordering::SeqCst) {
            1
        } else {
            0
        };
    }
    // Only emit intermediate denoise previews (and final frames when denoise_step < 0 skip bulk).
    if f.denoise_step >= 0 {
        if let Some(b64) = rgb_to_jpeg_base64(f) {
            let _ = bridge.tx.send(ProgressEvent::Preview {
                jpeg_base64: b64,
                denoise_step: f.denoise_step,
                denoise_steps: f.denoise_steps,
                frame_index: f.frame_index,
                frame_count: f.frame_count,
                width: f.width,
                height: f.height,
            });
        }
    }
    if bridge.cancel.load(Ordering::SeqCst) {
        1
    } else {
        0
    }
}

fn rgb_to_jpeg_base64(f: &h3_frame) -> Option<String> {
    let w = f.width as u32;
    let h = f.height as u32;
    let stride = f.stride as usize;
    let mut rgb = vec![0u8; (w * h * 3) as usize];
    unsafe {
        for y in 0..h as usize {
            let src = f.rgb.add(y * stride);
            let dst = &mut rgb[y * (w as usize) * 3..(y + 1) * (w as usize) * 3];
            ptr::copy_nonoverlapping(src, dst.as_mut_ptr(), (w as usize) * 3);
        }
    }
    let img = image::RgbImage::from_raw(w, h, rgb)?;
    let mut cursor = std::io::Cursor::new(Vec::new());
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, 80);
    encoder.encode_image(&img).ok()?;
    Some(base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        cursor.into_inner(),
    ))
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceKind {
    Image,
    Video,
    Audio,
    VideoAudio,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ReferenceSpec {
    pub kind: ReferenceKind,
    pub path: String,
    #[serde(default)]
    pub audio_path: Option<String>,
    #[serde(default)]
    pub include_embedded_audio: bool,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct GenerateRequest {
    pub prompt: String,
    #[serde(default = "default_width")]
    pub width: i32,
    #[serde(default = "default_height")]
    pub height: i32,
    #[serde(default)]
    pub frames: Option<i32>,
    #[serde(default)]
    pub seconds: Option<f64>,
    #[serde(default = "default_steps")]
    pub steps: i32,
    #[serde(default = "default_seed")]
    pub seed: u64,
    #[serde(default)]
    pub output_name: Option<String>,
    #[serde(default)]
    pub first_frame: Option<String>,
    #[serde(default)]
    pub last_frame: Option<String>,
    #[serde(default)]
    pub references: Vec<ReferenceSpec>,
    #[serde(default = "default_ref_image_size")]
    pub reference_image_size: String,
    #[serde(default = "default_one")]
    pub denoise_reuse: i32,
    #[serde(default = "default_dit_layers")]
    pub dit_layers: i32,
    #[serde(default = "default_one")]
    pub core_reuse: i32,
    #[serde(default)]
    pub token_reduction: bool,
    #[serde(default)]
    pub veda: bool,
    #[serde(default)]
    pub turbo_lora: bool,
    #[serde(default)]
    pub ssd_streaming: bool,
    #[serde(default)]
    pub use_int8_row_fc2: bool,
    #[serde(default)]
    pub use_reference_rope: bool,
    #[serde(default)]
    pub use_slower_bf16_mlp: bool,
    #[serde(default)]
    pub use_slower_bf16_qkv: bool,
    #[serde(default)]
    pub use_slower_bf16_attention_output: bool,
    #[serde(default)]
    pub use_slower_row_major_attention_output: bool,
    #[serde(default)]
    pub use_slower_unfused_int8_inputs: bool,
    #[serde(default)]
    pub use_slower_unfused_qkv_rope: bool,
    #[serde(default)]
    pub use_slower_scalar_qkv_rms: bool,
    #[serde(default)]
    pub use_slower_uncached_int8_scales: bool,
    #[serde(default)]
    pub use_slower_dynamic_fc1_k: bool,
    #[serde(default)]
    pub use_slower_grouped_quantizer: bool,
    #[serde(default)]
    pub render_width: i32,
    #[serde(default)]
    pub render_height: i32,
    #[serde(default)]
    pub preview_denoise: bool,
    #[serde(default)]
    pub frames_dir: Option<String>,
}

fn default_width() -> i32 {
    H3_DEFAULT_WIDTH as i32
}
fn default_height() -> i32 {
    H3_DEFAULT_HEIGHT as i32
}
fn default_steps() -> i32 {
    H3_DEFAULT_STEPS as i32
}
fn default_seed() -> u64 {
    42
}
fn default_one() -> i32 {
    1
}
fn default_dit_layers() -> i32 {
    H3_DEFAULT_DIT_LAYERS as i32
}
fn default_ref_image_size() -> String {
    "match".into()
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct GenerateResult {
    pub width: i32,
    pub height: i32,
    pub frames: i32,
    pub fps: i32,
    pub sample_rate: i32,
    pub seed: u64,
    pub output_path: Option<String>,
}

pub struct Engine {
    ctx: Mutex<Option<*mut h3_ctx>>,
    model_dir: Mutex<Option<PathBuf>>,
}

// h3_ctx is owned by the engine and only touched on the generate thread / under mutex.
unsafe impl Send for Engine {}
unsafe impl Sync for Engine {}

impl Engine {
    pub fn new() -> Self {
        Self {
            ctx: Mutex::new(None),
            model_dir: Mutex::new(None),
        }
    }

    pub fn version() -> &'static str {
        std::str::from_utf8(H3_VERSION)
            .unwrap_or("unknown")
            .trim_end_matches('\0')
    }

    pub fn ensure_loaded(&self, model_dir: &Path, cache_enabled: bool) -> Result<(), EngineError> {
        let mut guard = self.ctx.lock();
        let mut dir_guard = self.model_dir.lock();
        if let (Some(_), Some(cur)) = (*guard, dir_guard.as_ref()) {
            if cur == model_dir {
                unsafe {
                    if let Some(ctx) = *guard {
                        h3_cache_set_enabled(ctx, if cache_enabled { 1 } else { 0 });
                    }
                }
                return Ok(());
            }
        }
        if let Some(old) = guard.take() {
            unsafe { h3_free(old) };
        }
        if !model_dir.is_dir() {
            return Err(EngineError::MissingModel(model_dir.to_path_buf()));
        }
        let fs = model_status_from_dir(model_dir);
        if !fs.fl2va_ready {
            return Err(EngineError::Message(format!(
                "FL2VA incomplete under {}: missing {}",
                model_dir.display(),
                fs.missing_paths.join(", ")
            )));
        }
        let c_dir = CString::new(model_dir.to_string_lossy().as_bytes())
            .map_err(|e| EngineError::Message(e.to_string()))?;
        let ctx = unsafe { h3_load_dir(c_dir.as_ptr()) };
        if ctx.is_null() {
            return Err(EngineError::Message(
                "h3_load_dir failed (null context)".into(),
            ));
        }
        let err = unsafe { h3_last_error(ctx) };
        if !err.is_null() {
            let msg = unsafe { CStr::from_ptr(err) }.to_string_lossy().into_owned();
            if !msg.is_empty() {
                // load_dir may set error on failure; check inventory via model
            }
        }
        // Verify FL2VA inventory is present
        let model = unsafe { h3_model(ctx) };
        if model.is_null() || unsafe { (*model).fl2va_transformer.files } == 0 {
            let msg = last_error_string(ctx).unwrap_or_else(|| "FL2VA model incomplete".into());
            unsafe { h3_free(ctx) };
            return Err(EngineError::Message(msg));
        }
        unsafe {
            h3_cache_set_enabled(ctx, if cache_enabled { 1 } else { 0 });
        }
        *guard = Some(ctx);
        *dir_guard = Some(model_dir.to_path_buf());
        Ok(())
    }

    pub fn unload(&self) {
        let mut guard = self.ctx.lock();
        if let Some(ctx) = guard.take() {
            unsafe { h3_free(ctx) };
        }
        *self.model_dir.lock() = None;
    }

    /// True when a context is already resident — never triggers a load.
    pub fn is_loaded(&self) -> bool {
        self.ctx.lock().is_some()
    }

    pub fn device_info(&self) -> Result<DeviceInfo, EngineError> {
        let guard = self.ctx.lock();
        let ctx = guard.ok_or_else(|| EngineError::Message("model not loaded".into()))?;
        let d = unsafe { h3_device(ctx) };
        if d.is_null() {
            return Err(EngineError::Message("h3_device returned null".into()));
        }
        let d = unsafe { &*d };
        Ok(DeviceInfo {
            name: c_array_to_string(&d.name),
            architecture: c_array_to_string(&d.architecture),
            physical_memory: d.physical_memory,
            recommended_working_set: d.recommended_working_set,
            max_buffer_length: d.max_buffer_length,
            apple_gpu_family: d.apple_gpu_family,
            metal4: d.metal4 != 0,
            unified_memory: d.unified_memory != 0,
        })
    }

    pub fn model_info(&self) -> Result<ModelInfo, EngineError> {
        let guard = self.ctx.lock();
        let ctx = guard.ok_or_else(|| EngineError::Message("model not loaded".into()))?;
        let m = unsafe { h3_model(ctx) };
        if m.is_null() {
            return Err(EngineError::Message("h3_model returned null".into()));
        }
        let m = unsafe { &*m };
        Ok(ModelInfo {
            text_encoder: component(&m.text_encoder),
            fl2va_transformer: component(&m.fl2va_transformer),
            ref2va_transformer: component(&m.ref2va_transformer),
            video_vae: component(&m.video_vae),
            audio_vae: component(&m.audio_vae),
        })
    }

    pub fn cache_info(&self) -> Result<CacheInfo, EngineError> {
        let guard = self.ctx.lock();
        let ctx = guard.ok_or_else(|| EngineError::Message("model not loaded".into()))?;
        let mut info = h3_sys::h3_cache_info {
            embedding_entries: 0,
            embedding_bytes: 0,
            prepared_dit: 0,
            video_decoder: 0,
        };
        unsafe { h3_cache_get_info(ctx, &mut info) };
        Ok(CacheInfo {
            embedding_entries: info.embedding_entries as u64,
            embedding_bytes: info.embedding_bytes as u64,
            prepared_dit: info.prepared_dit != 0,
            video_decoder: info.video_decoder != 0,
        })
    }

    pub fn clear_cache(&self) {
        let guard = self.ctx.lock();
        if let Some(ctx) = *guard {
            unsafe { h3_cache_clear(ctx) };
        }
    }

    /// Run generation on the calling thread (use a dedicated OS thread from the API).
    pub fn generate(
        &self,
        req: &GenerateRequest,
        output_dir: &Path,
        cancel: Arc<AtomicBool>,
        progress_tx: std::sync::mpsc::Sender<ProgressEvent>,
    ) -> Result<GenerateResult, EngineError> {
        let guard = self.ctx.lock();
        let ctx = guard.ok_or_else(|| EngineError::Message("model not loaded".into()))?;

        let frames = if let Some(s) = req.seconds {
            (s * 24.0).round() as i32
        } else {
            req.frames.unwrap_or(H3_DEFAULT_FRAMES as i32)
        };

        let output_path = {
            let name = req
                .output_name
                .clone()
                .unwrap_or_else(|| format!("h3-{}.mp4", chrono::Utc::now().format("%Y%m%d-%H%M%S")));
            let name = if name.ends_with(".mp4") {
                name
            } else {
                format!("{name}.mp4")
            };
            output_dir.join(name)
        };
        std::fs::create_dir_all(output_dir)
            .map_err(|e| EngineError::Message(e.to_string()))?;

        let prompt = CString::new(req.prompt.as_str())
            .map_err(|e| EngineError::Message(format!("invalid prompt: {e}")))?;
        let output_c = CString::new(output_path.to_string_lossy().as_bytes())
            .map_err(|e| EngineError::Message(e.to_string()))?;

        let first_c = req
            .first_frame
            .as_ref()
            .map(|p| CString::new(p.as_str()))
            .transpose()
            .map_err(|e| EngineError::Message(e.to_string()))?;
        let last_c = req
            .last_frame
            .as_ref()
            .map(|p| CString::new(p.as_str()))
            .transpose()
            .map_err(|e| EngineError::Message(e.to_string()))?;
        let frames_dir_c = req
            .frames_dir
            .as_ref()
            .map(|p| CString::new(p.as_str()))
            .transpose()
            .map_err(|e| EngineError::Message(e.to_string()))?;

        // Keep CStrings alive for references
        let mut ref_path_cstrs: Vec<CString> = Vec::new();
        let mut ref_audio_cstrs: Vec<Option<CString>> = Vec::new();
        let mut references: Vec<h3_reference> = Vec::new();

        for r in &req.references {
            let path = CString::new(r.path.as_str())
                .map_err(|e| EngineError::Message(e.to_string()))?;
            let audio = r
                .audio_path
                .as_ref()
                .map(|p| CString::new(p.as_str()))
                .transpose()
                .map_err(|e| EngineError::Message(e.to_string()))?;
            let kind = match r.kind {
                ReferenceKind::Image => h3_reference_kind_H3_REFERENCE_IMAGE,
                ReferenceKind::Video => h3_reference_kind_H3_REFERENCE_VIDEO,
                ReferenceKind::Audio => h3_reference_kind_H3_REFERENCE_AUDIO,
                ReferenceKind::VideoAudio => h3_reference_kind_H3_REFERENCE_VIDEO_AUDIO,
            };
            let path_ptr = path.as_ptr();
            let audio_ptr = audio.as_ref().map(|a| a.as_ptr()).unwrap_or(ptr::null());
            ref_path_cstrs.push(path);
            ref_audio_cstrs.push(audio);
            references.push(h3_reference {
                kind,
                path: path_ptr,
                audio_path: audio_ptr,
                include_embedded_audio: if r.include_embedded_audio { 1 } else { 0 },
            });
        }

        let ref_image_size = if req.reference_image_size == "max" {
            h3_reference_image_size_H3_REFERENCE_IMAGE_MAX
        } else {
            h3_reference_image_size_H3_REFERENCE_IMAGE_MATCH
        };

        let bridge = Box::new(CallbackBridge {
            cancel: cancel.clone(),
            tx: progress_tx,
        });
        let bridge_ptr = Box::into_raw(bridge) as *mut c_void;

        let mut params: h3_params = unsafe { std::mem::zeroed() };
        params.width = req.width;
        params.height = req.height;
        params.frames = frames;
        params.steps = req.steps;
        params.seed = req.seed;
        params.output_path = output_c.as_ptr();
        params.first_frame = first_c
            .as_ref()
            .map(|c| c.as_ptr())
            .unwrap_or(ptr::null());
        params.last_frame = last_c.as_ref().map(|c| c.as_ptr()).unwrap_or(ptr::null());
        params.references = if references.is_empty() {
            ptr::null()
        } else {
            references.as_ptr()
        };
        params.reference_count = references.len();
        params.reference_image_size = ref_image_size;
        params.denoise_reuse = req.denoise_reuse;
        params.dit_layers = req.dit_layers;
        params.core_reuse = req.core_reuse;
        params.token_reduction = if req.token_reduction { 1 } else { 0 };
        params.veda = if req.veda { 1 } else { 0 };
        params.turbo_lora = if req.turbo_lora { 1 } else { 0 };
        params.use_int8_row_fc2 = if req.use_int8_row_fc2 { 1 } else { 0 };
        params.use_reference_rope = if req.use_reference_rope { 1 } else { 0 };
        params.ssd_streaming = if req.ssd_streaming { 1 } else { 0 };
        params.render_width = req.render_width;
        params.render_height = req.render_height;
        params.use_slower_bf16_mlp = if req.use_slower_bf16_mlp { 1 } else { 0 };
        params.use_slower_bf16_qkv = if req.use_slower_bf16_qkv { 1 } else { 0 };
        params.use_slower_bf16_attention_output =
            if req.use_slower_bf16_attention_output { 1 } else { 0 };
        params.use_slower_row_major_attention_output =
            if req.use_slower_row_major_attention_output {
                1
            } else {
                0
            };
        params.use_slower_unfused_int8_inputs =
            if req.use_slower_unfused_int8_inputs { 1 } else { 0 };
        params.use_slower_unfused_qkv_rope =
            if req.use_slower_unfused_qkv_rope { 1 } else { 0 };
        params.use_slower_scalar_qkv_rms = if req.use_slower_scalar_qkv_rms { 1 } else { 0 };
        params.use_slower_uncached_int8_scales =
            if req.use_slower_uncached_int8_scales { 1 } else { 0 };
        params.use_slower_dynamic_fc1_k = if req.use_slower_dynamic_fc1_k { 1 } else { 0 };
        params.use_slower_grouped_quantizer =
            if req.use_slower_grouped_quantizer { 1 } else { 0 };
        params.preview_denoise = if req.preview_denoise { 1 } else { 0 };
        params.on_progress = Some(on_progress_cb);
        params.on_frame = if req.preview_denoise || frames_dir_c.is_some() {
            Some(on_frame_cb)
        } else {
            None
        };
        params.callback_opaque = bridge_ptr;
        // frames_dir is CLI-only via on_frame writing PPM; not in h3_params.
        // We only use on_frame for preview when preview_denoise is set.
        let _ = frames_dir_c;

        let result_ptr = unsafe { h3_generate(ctx, prompt.as_ptr(), &params) };

        // Free bridge
        unsafe {
            drop(Box::from_raw(bridge_ptr as *mut CallbackBridge));
        }

        if cancel.load(Ordering::SeqCst) {
            if !result_ptr.is_null() {
                unsafe { h3_result_free(result_ptr) };
            }
            return Err(EngineError::Cancelled);
        }

        if result_ptr.is_null() {
            let msg = last_error_string(ctx).unwrap_or_else(|| "h3_generate failed".into());
            return Err(EngineError::Message(msg));
        }

        let r = unsafe { &*result_ptr };
        let out = GenerateResult {
            width: r.width,
            height: r.height,
            frames: r.frames,
            fps: r.fps,
            sample_rate: r.sample_rate,
            seed: r.seed,
            output_path: Some(output_path.to_string_lossy().into_owned()),
        };
        unsafe { h3_result_free(result_ptr) };
        Ok(out)
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.unload();
    }
}

fn last_error_string(ctx: *mut h3_ctx) -> Option<String> {
    let err = unsafe { h3_last_error(ctx) };
    if err.is_null() {
        return None;
    }
    let s = unsafe { CStr::from_ptr(err) }
        .to_string_lossy()
        .into_owned();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn c_array_to_string(buf: &[c_char]) -> String {
    let bytes: Vec<u8> = buf
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn component(c: &h3_sys::h3_component_info) -> ComponentInfo {
    ComponentInfo {
        bytes: c.bytes,
        tensor_bytes: c.tensor_bytes,
        files: c.files as u64,
        tensors: c.tensors as u64,
    }
}

/// Filesystem readiness without loading Metal weights.
pub fn model_status_from_dir(model_dir: &Path) -> ModelStatusFs {
    let fl2va_required = [
        "FL2VA/transformer/config.json",
        "FL2VA/tokenizer/tokenizer.json",
    ];
    let fl2va_dirs = [
        "FL2VA/text_encoder",
        "FL2VA/transformer",
        "FL2VA/video_vae/source",
        "FL2VA/audio_vae",
    ];
    let mut missing = Vec::new();
    for rel in fl2va_required {
        if !model_dir.join(rel).is_file() {
            missing.push(rel.to_string());
        }
    }
    for rel in fl2va_dirs {
        let p = model_dir.join(rel);
        if !p.is_dir() || dir_is_empty(&p) {
            missing.push(rel.to_string());
        }
    }
    let ref2va = model_dir
        .join("Ref2VA/transformer/model.safetensors.index.json")
        .is_file();
    let turbo_lora = turbo_lora_ready(model_dir);
    let disk_bytes = dir_size(model_dir).unwrap_or(0);
    ModelStatusFs {
        model_dir: model_dir.to_string_lossy().into_owned(),
        fl2va_ready: missing.is_empty(),
        ref2va_ready: ref2va,
        turbo_lora_ready: turbo_lora,
        missing_paths: missing,
        disk_bytes,
    }
}

fn turbo_lora_ready(model_dir: &Path) -> bool {
    // Keep in sync with crate::download::TURBO_LORA_PREFERRED (avoid cycle).
    const PREFERRED: &str = "minimax_h3_turbo_v4_step600_ema.safetensors";
    let preferred = model_dir.join("turbo_lora").join(PREFERRED);
    if preferred.is_file() {
        return true;
    }
    for rel in [
        "turbo_lora",
        "FL2VA/turbo_lora",
        "Ref2VA/turbo_lora",
    ] {
        let path = model_dir.join(rel);
        if path.is_file()
            && path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("safetensors"))
        {
            return true;
        }
        if path.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&path) {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    if name.ends_with(".safetensors") {
                        return true;
                    }
                }
            }
        }
    }
    false
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelStatusFs {
    pub model_dir: String,
    pub fl2va_ready: bool,
    pub ref2va_ready: bool,
    pub turbo_lora_ready: bool,
    pub missing_paths: Vec<String>,
    pub disk_bytes: u64,
}

fn dir_is_empty(path: &Path) -> bool {
    std::fs::read_dir(path)
        .map(|mut d| d.next().is_none())
        .unwrap_or(true)
}

pub fn dir_size(path: &Path) -> std::io::Result<u64> {
    let mut total = 0u64;
    if !path.exists() {
        return Ok(0);
    }
    fn walk(p: &Path, total: &mut u64) -> std::io::Result<()> {
        if p.is_file() {
            *total += p.metadata()?.len();
            return Ok(());
        }
        if p.is_dir() {
            for e in std::fs::read_dir(p)? {
                walk(&e?.path(), total)?;
            }
        }
        Ok(())
    }
    walk(path, &mut total)?;
    Ok(total)
}

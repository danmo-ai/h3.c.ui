export type DownloadSourceId = 'huggingface' | 'hf_mirror' | 'modelscope';
export type DownloadPackageId = 'fl2va' | 'ref2va' | 'turbo_lora' | 'all';

export type ModelStatus = {
  model_dir: string;
  fl2va_ready: boolean;
  ref2va_ready: boolean;
  turbo_lora_ready?: boolean;
  missing_paths: string[];
  disk_bytes: number;
  loaded: boolean;
  hf_repo: string;
  modelscope_repo?: string;
  turbo_lora_hf_repo?: string;
  turbo_lora_modelscope_repo?: string;
  download_source?: DownloadSourceId;
  sources?: Array<{ id: DownloadSourceId; repo: string; endpoint?: string | null }>;
  model?: unknown;
};

export type GenerateBody = {
  prompt: string;
  width: number;
  height: number;
  frames?: number | null;
  seconds?: number | null;
  steps: number;
  seed: number;
  output_name?: string | null;
  first_frame?: string | null;
  last_frame?: string | null;
  references: Array<{
    kind: 'image' | 'video' | 'audio' | 'video_audio';
    path: string;
    audio_path?: string | null;
    include_embedded_audio?: boolean;
  }>;
  reference_image_size: string;
  denoise_reuse: number;
  dit_layers: number;
  core_reuse: number;
  token_reduction: boolean;
  veda: boolean;
  turbo_lora: boolean;
  ssd_streaming: boolean;
  use_int8_row_fc2: boolean;
  use_reference_rope: boolean;
  use_slower_bf16_mlp: boolean;
  use_slower_bf16_qkv: boolean;
  use_slower_bf16_attention_output: boolean;
  use_slower_row_major_attention_output: boolean;
  use_slower_unfused_int8_inputs: boolean;
  use_slower_unfused_qkv_rope: boolean;
  use_slower_scalar_qkv_rms: boolean;
  use_slower_uncached_int8_scales: boolean;
  use_slower_dynamic_fc1_k: boolean;
  use_slower_grouped_quantizer: boolean;
  render_width: number;
  render_height: number;
  preview_denoise: boolean;
  frames_dir?: string | null;
};

export type TaskSnapshot = {
  task_id: string;
  status: 'queued' | 'running' | 'completed' | 'failed' | 'cancelled';
  phase: string;
  completed: number;
  total: number;
  progress: number;
  preview_jpeg_base64?: string | null;
  result?: {
    width: number;
    height: number;
    frames: number;
    fps: number;
    sample_rate: number;
    seed: number;
    output_path?: string | null;
  } | null;
  error?: string | null;
  prompt?: string;
  output_name?: string | null;
  width?: number;
  height?: number;
  frames?: number | null;
  seconds?: number | null;
  created_ms?: number;
  finished_ms?: number | null;
};

export type DownloadProgress = {
  task_id: string;
  status: 'queued' | 'running' | 'completed' | 'failed' | 'cancelled' | 'idle';
  progress: number;
  downloaded_bytes: number;
  total_bytes?: number | null;
  files_done?: number;
  files_total?: number;
  current_file?: string | null;
  speed_bps: number;
  message: string;
  error?: string | null;
  source?: string;
  repo?: string;
  package?: string;
};

async function json<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, {
    ...init,
    headers: {
      'Content-Type': 'application/json',
      ...(init?.headers || {}),
    },
  });
  if (!res.ok) {
    const text = await res.text();
    throw new Error(text || res.statusText);
  }
  return res.json() as Promise<T>;
}

/** Wait until h3-api answers (Tauri sidecar often boots after Vite). */
export async function waitForApi(opts?: {
  timeoutMs?: number;
  intervalMs?: number;
}): Promise<boolean> {
  const timeoutMs = opts?.timeoutMs ?? 60_000;
  const intervalMs = opts?.intervalMs ?? 250;
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    try {
      const res = await fetch('/api/system/health', { method: 'GET' });
      if (res.ok) return true;
    } catch {
      /* ECONNREFUSED while sidecar starts */
    }
    await new Promise((r) => setTimeout(r, intervalMs));
  }
  return false;
}

export const api = {
  health: () => json<{ ok: boolean; engine_version: string }>('/api/system/health'),
  ffmpeg: () =>
    json<{
      ffmpeg: boolean;
      ffprobe: boolean;
      ffmpeg_path?: string | null;
      ffprobe_path?: string | null;
      source?: string | null;
    }>('/api/system/ffmpeg'),
  modelStatus: () => json<ModelStatus>('/api/model/status'),
  modelLoad: () => json<{ ok: boolean }>('/api/model/load', { method: 'POST' }),
  startDownload: (source?: DownloadSourceId, packageId: DownloadPackageId = 'fl2va') =>
    json<{ task_id: string; source: string; repo: string; package: string }>(
      '/api/model/download',
      {
        method: 'POST',
        body: JSON.stringify({ source, package: packageId }),
      },
    ),
  downloadCurrent: () => json<DownloadProgress>('/api/model/download/current'),
  cancelDownload: (taskId: string) =>
    json<{ ok: boolean }>(`/api/model/download/${taskId}/cancel`, { method: 'POST' }),
  downloadStreamUrl: (taskId: string) => `/api/model/download/${taskId}/stream`,
  generate: (body: GenerateBody) =>
    json<{ task_id: string }>('/api/generate', {
      method: 'POST',
      body: JSON.stringify(body),
    }),
  taskStreamUrl: (taskId: string) => `/api/tasks/${taskId}/stream`,
  cancelTask: (taskId: string) =>
    json<{ ok: boolean }>(`/api/tasks/${taskId}/cancel`, { method: 'POST' }),
  getTask: (taskId: string) => json<TaskSnapshot>(`/api/tasks/${taskId}`),
  listFailedTasks: () => json<{ tasks: TaskSnapshot[] }>('/api/tasks/failed'),
  listOutputs: () =>
    json<{
      outputs: Array<{ name: string; path: string; bytes: number; modified?: number }>;
    }>('/api/outputs'),
  deleteOutput: (name: string) =>
    json<{ ok: boolean }>(`/api/outputs/${encodeURIComponent(name)}`, { method: 'DELETE' }),
  outputFileUrl: (name: string) => `/api/outputs/${encodeURIComponent(name)}/file`,
  getSettings: () =>
    json<{
      home: string;
      model_dir: string;
      output_dir: string;
      cache_enabled: boolean;
      hf_repo: string;
    }>('/api/settings'),
  putSettings: (body: {
    model_dir?: string;
    output_dir?: string;
    cache_enabled?: boolean;
    hf_repo?: string;
  }) =>
    json<{ ok: boolean }>('/api/settings', {
      method: 'PUT',
      body: JSON.stringify(body),
    }),
  uploadFile: async (file: File) => {
    const fd = new FormData();
    fd.append('file', file, file.name);
    const res = await fetch('/api/uploads', { method: 'POST', body: fd });
    if (!res.ok) {
      const text = await res.text();
      throw new Error(text || res.statusText);
    }
    return res.json() as Promise<{
      name: string;
      path: string;
      bytes: number;
      content_type: string;
      original_name: string;
      kind: string;
    }>;
  },
  uploadFileUrl: (name: string) => `/api/uploads/${encodeURIComponent(name)}/file`,
};

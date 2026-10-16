# H3.UI

Language: **English** | [中文](README_zh.md)

[![Release](https://img.shields.io/github/v/release/danmo-ai/h3.c.ui?label=release)](https://github.com/danmo-ai/h3.c.ui/releases/latest)
[![CI](https://img.shields.io/github/actions/workflow/status/danmo-ai/h3.c.ui/ci.yml?branch=main&label=CI)](https://github.com/danmo-ai/h3.c.ui/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/danmo-ai/h3.c.ui)](LICENSE)

**Native MiniMax-H3 video on Apple Silicon** — builds on [antirez/h3.c](https://github.com/antirez/h3.c) (Metal CLI engine) with a macOS **H3.UI** product shell: bilingual Create studio, one-click model packs, and REST/SSE control plane under `~/.h3-ui/`.

![H3.UI Create studio](docs/screenshots/create.png)

Product name: **H3.UI**. Engine CLI stays `./h3` (same lineage as upstream). Control plane: `~/.h3-ui/`.

| | |
|---|---|
| **Install** | [GitHub Releases](https://github.com/danmo-ai/h3.c.ui/releases/latest) · `H3.UI_*_aarch64.dmg` |
| **Upstream engine** | [antirez/h3.c](https://github.com/antirez/h3.c) — Metal CLI, presets, kernels |
| **Engine deep-dive** | [ENGINE.md](ENGINE.md) (CLI flags carried from upstream + fork additions) |
| **Perf port notes** | [docs/PERF_PORT.md](docs/PERF_PORT.md) (Flash / Veda / Morton A/B on M5) |
| **UI / API layout** | [UI.md](UI.md) |
| **Sibling design system** | [dq-ui](https://github.com/danmo-ai/dq-ui) |

Studio pattern matches [Danmo Make](https://github.com/danmo-ai/danmo-make): icon rail · card gallery · composer drawer · bilingual UI.

---

## From original h3.c (kept)

Upstream [antirez/h3.c](https://github.com/antirez/h3.c) already delivers the Metal core. This repo still ships it as `libh3` + `./h3`:

| Upstream baseline | Still here |
|-------------------|------------|
| On-device Metal DiT | `libh3.dylib` + Apple frameworks |
| Prompt / FL2VA / Ref2VA | Same generation paths |
| Interactive CLI | `./h3` session (`!first`, `!ref-image`, profiles…) |
| Speed / memory knobs | SSD streaming · token reduction · int8 row FC2 · Reference RoPE · layers / reuse / `--show` |

Full CLI tutorial: [ENGINE.md](ENGINE.md).

---

## Extensions in this repo (vs original h3.c)

What **H3.UI / h3.c.ui** adds on top of the upstream CLI-only tree:

### Product shell
| Extension | Detail |
|-----------|--------|
| **Desktop app** | Tauri 2 macOS shell · DMG release · bundled static `ffmpeg` / `ffprobe` |
| **Create studio** | Vue 3 + `@danqing/dq-ui` · Create · Download · Settings · en/zh |
| **Local API** | Rust `h3-api` (axum) over FFI — REST + SSE, not shelling out to `./h3` |
| **Control plane** | `~/.h3-ui/` config, uploads, outputs, download state |

### Engine additions (also exposed in CLI / env)
| Extension | Detail |
|-----------|--------|
| **Turbo LoRA** | Optional 8-step adapter (`larryvrh/MiniMax-H3-Turbo-Lora`, ~780 MiB); Create + `./h3 --turbo-lora` |
| **Veda sparse attention** | Tile-sparse SDPA (`--veda` / `!veda`); local keep-ratio plan or `H3_VEDA_PLAN`; **mutex** with token reduction |
| **FlashAttention (opt-in)** | Streamed BF16 / int8 head-major SDPA via `H3_FORCE_FLASH_ATTN=1` [`H3_FORCE_INT8_FLASH_ATTN=1`]; not default yet (slower than MPSGraph on widescreen A/B) |
| **Long-seq Morton tiles** | TensorOps linear schedule chunks past 2 048 rows (no 3 072 cap) for widescreen shapes |
| **Int8 MLP weight cache** | `H3_INT8_CACHE=1` [`H3_INT8_CACHE_DIR`] persists runtime-quantized FC weights across launches |
| **Module profile** | `H3_PROFILE=1` prints encode-side shares (`qkv` / `attn` / `attn_out` / `mlp`) |

Mutex pairs (engine rejects both on): **Veda ↔ token reduction**, **Turbo LoRA ↔ SSD streaming**. Create studio exposes each pair as a pick-one dropdown. Numbers and enable flags: [docs/PERF_PORT.md](docs/PERF_PORT.md).

### Ops & UX (not in upstream CLI)
| Extension | Detail |
|-----------|--------|
| **Mutex Create controls** | Attention path (off / token reduction / Veda) · Residence (off / Turbo LoRA / SSD streaming) |
| **Split model packs** | Download FL2VA · Ref2VA · Turbo LoRA separately (ModelScope / HF Mirror / Hugging Face) |
| **Resume / verify** | HTTP Range resume + SHA-256; cancel keeps progress |
| **Media upload** | First/last frames & Ref2VA refs as real file uploads (no path paste) |
| **Gallery** | Card canvas · lightbox (enlarge / prev·next / download) · delete · seekable `.mp4` |
| **Live preview** | Denoise middle-frame over SSE while a task runs |
| **Failed tasks** | Drawer + badge; `GET /api/tasks/failed` |
| **Settings** | Theme, language, model/output dirs, HF repo id, session weight cache |
| **CI / release** | `make pack` → GitHub Actions Apple Silicon `.dmg` |

---

## Requirements

| | |
|--|--|
| **Platform** | macOS **Apple Silicon** (primary) |
| **RAM** | 32 GB+ recommended for full MiniMax-H3 |
| **Disk** | Tens of GB for FL2VA (+ Ref2VA; + ~780 MiB Turbo LoRA) |
| **From source** | Xcode CLT, Rust, Node.js 20+, sibling [dq-ui](https://github.com/danmo-ai/dq-ui) |

Windows / Linux desktop packs are not shipped yet. The Metal engine targets Apple Silicon.

---

## Install from release (recommended)

1. Download **`H3.UI_*_aarch64.dmg`** from [Releases](https://github.com/danmo-ai/h3.c.ui/releases/latest).
2. Open the DMG and drag **H3.UI** into Applications.
3. First launch may be blocked (unsigned / quarantine). Prefer **Control-click → Open**, or:

```bash
xattr -dr com.apple.quarantine "/Applications/H3.UI.app"
```

4. In the app: **Download** → pick a source (ModelScope recommended in China) → fetch **FL2VA** (and **Ref2VA** / **Turbo LoRA** if you need them) → **Create**.

---

## Quick start (from source)

For contributors. End users: use **Install from release** above.

```bash
# sibling design system (required for frontend)
git clone https://github.com/danmo-ai/dq-ui.git
cd dq-ui && npm install && npm run build && cd ..

git clone https://github.com/danmo-ai/h3.c.ui.git
cd h3.c.ui

make ui      # desktop (fetches FFmpeg, builds API + Tauri)
# make web  # browser UI :5810 + API :7810
# make pack # .app / .dmg
```

| Product | Backend | Frontend |
|---------|---------|----------|
| **H3.UI (this repo)** | 7810 | 5810 |
| [Danmo Make](https://github.com/danmo-ai/danmo-make) | 7800 | 5800 |
| [Danmo Work](https://github.com/danmo-ai/danmo-work) | 7801 | 5801 |

Engine-only CLI (no UI):

```bash
make -j8
./h3 --info -d ./MiniMax-H3   # or weights under ~/.h3-ui/models/MiniMax-H3
```

Full CLI tutorial and Metal notes: [ENGINE.md](ENGINE.md).

---

## Data layout

| Path | Role |
|------|------|
| `~/.h3-ui/` | Control plane: `config.json`, downloads, uploads |
| `~/.h3-ui/models/MiniMax-H3/` | Weights (FL2VA / Ref2VA trees + Turbo LoRA adapter) |
| `~/.h3-ui/outputs/` | Generated `.mp4` |
| `~/.h3-ui/uploads/` | First/last frames & reference media |

---

## Architecture

```
H3.UI (Tauri)  ──►  Vue SPA (@danqing/dq-ui)
                         │  /api (proxy)
                         ▼
                   h3-api (Rust / axum + SSE)
                         │  FFI
                         ▼
                   libh3.dylib  (Metal MiniMax-H3)
                         │
              bundled ffmpeg / ffprobe
```

| Path | Role |
|------|------|
| `libh3` / `h3.h` / `./h3` | Native engine + CLI |
| `crates/h3-sys` | bindgen FFI |
| `crates/h3-api` | REST + SSE sidecar |
| `frontend/` | Vue 3 studio |
| `desktop/` | Tauri 2 macOS shell |

---

## API (local)

Base: `http://127.0.0.1:7810`

| Area | Endpoints |
|------|-----------|
| System | `GET /api/system/health` · `device` · `ffmpeg` |
| Model | `GET /api/model/status` · `POST /api/model/download` + SSE (`package`: `fl2va` · `ref2va` · `turbo_lora` · `all`) |
| Generate | `POST /api/generate` · task SSE `/api/tasks/{id}/stream` · cancel |
| Tasks | `GET /api/tasks/failed` |
| Uploads | `POST /api/uploads` · `GET /api/uploads/{name}/file` |
| Outputs | `GET/DELETE /api/outputs…` (file GET supports `Range`) |
| Settings | `GET/PUT /api/settings` |

Download sources: `modelscope` · `hf_mirror` · `huggingface`.

---

## Release / CI

Tag `v*` → [`.github/workflows/release.yml`](.github/workflows/release.yml) packs macOS `.dmg` on Apple Silicon runners and publishes a GitHub Release.

```bash
git tag v0.1.1
git push origin v0.1.1
```

`main` / PRs run [`.github/workflows/ci.yml`](.github/workflows/ci.yml) (`make pack`).

---

## Related

- [antirez/h3.c](https://github.com/antirez/h3.c) — original Metal MiniMax-H3 CLI (upstream baseline)
- [Danmo Make](https://github.com/danmo-ai/danmo-make) — local image / video / audio studio (MLX)
- [Danmo Work](https://github.com/danmo-ai/danmo-work) — open-source AI work agent
- [dq-ui](https://github.com/danmo-ai/dq-ui) — shared Vue design system
- [MiniMax-H3](https://huggingface.co/MiniMaxAI/MiniMax-H3) — model weights

---

## License

[MIT](LICENSE)

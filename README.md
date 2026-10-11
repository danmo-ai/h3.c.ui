# H3.UI

Language: **English** | [中文](README_zh.md)

[![Release](https://img.shields.io/github/v/release/danmo-ai/h3.c.ui?label=release)](https://github.com/danmo-ai/h3.c.ui/releases/latest)
[![CI](https://img.shields.io/github/actions/workflow/status/danmo-ai/h3.c.ui/ci.yml?branch=main&label=CI)](https://github.com/danmo-ai/h3.c.ui/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/danmo-ai/h3.c.ui)](LICENSE)

**Native MiniMax-H3 video on Apple Silicon** — Metal-first `libh3` engine, macOS desktop app, and a bilingual Create studio. Prompt → video/audio, first/last-frame, and ordered Ref2VA references, without leaving your Mac.

Product name: **H3.UI**. Engine CLI stays `h3`; control plane lives under `~/.h3-ui/`.

| | |
|---|---|
| **Install** | [GitHub Releases](https://github.com/danmo-ai/h3.c.ui/releases/latest) · `H3.UI_*_aarch64.dmg` |
| **Engine deep-dive** | [ENGINE.md](ENGINE.md) (CLI, Metal kernels, presets) |
| **UI / API layout** | [UI.md](UI.md) |
| **Sibling design system** | [dq-ui](https://github.com/danmo-ai/dq-ui) |

Same studio pattern as [Danmo Make](https://github.com/danmo-ai/danmo-make): icon rail · card gallery · composer drawer · bilingual UI.

---

## Why H3.UI

- **On-device Metal** — C / Objective-C `libh3` + Apple frameworks (Metal, MPS, Accelerate). No cloud GPU queue for generation.
- **Real product shell** — Tauri 2 + Vue 3 + `@danqing/dq-ui`. Create · Download · Settings, not a raw CLI wrapper.
- **FL2VA & Ref2VA** — Text-only, first/last keyframes (upload), or ordered image / video / audio references.
- **One-click model fetch** — ModelScope · HF Mirror · Hugging Face; HTTP Range resume + SHA-256 verify.
- **Bundled FFmpeg** — Static `ffmpeg` / `ffprobe` in the app; PATH is only a fallback.
- **Live denoise preview** — Watch the middle frame evolve while the task runs (SSE progress).

---

## Features

| Area | What you get |
|------|----------------|
| **Generate** | Prompt, resolution, frames/seconds, steps, seed, speed & compatibility toggles |
| **Keyframes** | Upload first (required in FL2VA mode) and optional last frame — real files, not path paste |
| **Ref2VA** | Ordered references: image / video / audio / video+audio |
| **Models** | Download MiniMax-H3 into `~/.h3-ui/models/`; status for FL2VA / Ref2VA readiness |
| **Gallery** | Local outputs as cards; open / delete |
| **Engine CLI** | Same repo: `./h3` interactive session, profiles, parity flags — see [ENGINE.md](ENGINE.md) |

---

## Requirements

| | |
|--|--|
| **Platform** | macOS **Apple Silicon** (primary) |
| **RAM** | 32 GB+ recommended for full MiniMax-H3 |
| **Disk** | Tens of GB for FL2VA (+ more for Ref2VA) |
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

4. In the app: **Download** → pick a source (ModelScope recommended in China) → wait until FL2VA (and Ref2VA if you need references) is ready → **Create**.

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
| `~/.h3-ui/models/MiniMax-H3/` | Weights (FL2VA / Ref2VA trees) |
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
| Model | `GET /api/model/status` · `POST /api/model/download` + SSE |
| Generate | `POST /api/generate` · task SSE `/api/tasks/{id}/stream` |
| Uploads | `POST /api/uploads` · `GET /api/uploads/{name}/file` |
| Outputs | `GET/DELETE /api/outputs…` |
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

- [Danmo Make](https://github.com/danmo-ai/danmo-make) — local image / video / audio studio (MLX)
- [Danmo Work](https://github.com/danmo-ai/danmo-work) — open-source AI work agent
- [dq-ui](https://github.com/danmo-ai/dq-ui) — shared Vue design system
- [MiniMax-H3](https://huggingface.co/MiniMaxAI/MiniMax-H3) — model weights

---

## License

[MIT](LICENSE)

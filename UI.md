# H3.UI — layout & commands

Product overview: [README.md](README.md) · [中文](README_zh.md)  
Engine CLI / Metal: [ENGINE.md](ENGINE.md)

Native Metal MiniMax-H3 (`libh3`) with a Tauri + Vue (`@danqing/dq-ui`) shell and
Rust REST API. Pattern matches Danmo Make: icon sidebar · card gallery · composer drawer.

## Layout

| Path | Role |
|------|------|
| `libh3.dylib` / `h3.h` / `./h3` | Engine + CLI (function calls for UI, not shelling out) |
| `crates/h3-sys` | bindgen FFI |
| `crates/h3-api` | axum REST + SSE (`:7810`) |
| `frontend/` | Vue 3 studio (`:5810`) |
| `desktop/` | Tauri 2 Mac shell |

Control plane: `~/.h3-ui/`. Frontend needs sibling [`dq-ui`](https://github.com/danmo-ai/dq-ui)
at `../dq-ui` (`npm install && npm run build` once).

## Commands

```bash
make ui      # desktop (recommended; fetches FFmpeg if needed)
make web     # browser :5810 + API :7810
make pack    # macOS .app / .dmg (bundles FFmpeg + FFprobe)
make ffmpeg  # download static FFmpeg/FFprobe → vendor/ffmpeg/
make smoke   # API smoke (no weights needed)
make help
```

## Release / CI

- `.github/workflows/ci.yml` — `make pack` on `main` / PRs  
- `.github/workflows/release.yml` — `v*` tags → `.dmg` + GitHub Release  

Latest: https://github.com/danmo-ai/h3.c.ui/releases/latest

## API surface

- `GET /api/system/health|device|ffmpeg`
- `GET /api/model/status` · `POST /api/model/download` `{ source }` + SSE  
  Sources: `modelscope` · `hf_mirror` · `huggingface` (Range resume + SHA-256)
- `POST /api/generate` → task SSE  
- `POST /api/uploads` · `GET /api/uploads/{name}/file`
- `GET/DELETE /api/outputs…` · `GET/PUT /api/settings`

Bundled FFmpeg: `H3_FFMPEG` / `H3_FFPROBE`; PATH is fallback only.

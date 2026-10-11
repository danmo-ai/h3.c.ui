# H3.UI

Native Metal MiniMax-H3 (`libh3`) with a Tauri + Vue (dq-ui) shell and Rust REST API.
Icon sidebar + card gallery + composer drawer (danmo-make pattern).

## Layout

| Path | Role |
|------|------|
| `libh3.dylib` / `h3.h` | Engine (function calls, not CLI) |
| `crates/h3-sys` | bindgen FFI |
| `crates/h3-api` | axum REST + SSE (`:7810`) |
| `frontend/` | Vue 3 + `@danqing/dq-*` (`:5810`) |
| `desktop/` | Tauri 2 Mac shell |

Control plane: `~/.h3-ui/`. Requires sibling [`dq-ui`](../dq-ui).

## Commands

```bash
make ui      # desktop (recommended; fetches FFmpeg if needed)
make web     # browser :5810 + API :7810
make pack    # macOS .app / .dmg (bundles FFmpeg + FFprobe)
make ffmpeg  # download static FFmpeg/FFprobe → vendor/ffmpeg/
make smoke   # API smoke (no weights needed)
make help
```

Requires sibling checkout [`danmo-ai/dq-ui`](https://github.com/danmo-ai/dq-ui) at `../dq-ui`
(build with `npm install && npm run build` there once).

## Release / CI

GitHub Actions (macOS):

- `.github/workflows/ci.yml` — build `make pack` on `main` / PRs
- `.github/workflows/release.yml` — on `v*` tags, upload `.dmg` and publish a GitHub Release

```bash
git tag v0.1.0
git push origin v0.1.0
```

## API surface

- `GET /api/system/health|device|ffmpeg`
- `GET /api/model/status` · `POST /api/model/download` `{ source }` + SSE  
  Sources: `modelscope` · `hf_mirror` · `huggingface` (pure HTTP; Range resume + SHA-256)
- `POST /api/generate` → task SSE (full `h3_params`)
- `GET/DELETE /api/outputs…` · `GET/PUT /api/settings`

`make pack` / `make ui` vendor static FFmpeg + FFprobe (`vendor/ffmpeg/…`,
copied into the app bundle). libh3 uses `H3_FFMPEG` / `H3_FFPROBE`; PATH is a
fallback only.

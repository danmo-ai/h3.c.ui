# H3 UI (dq-ui)

Shared packages live in the sibling repo [danmo-ai/dq-ui](https://github.com/danmo-ai/dq-ui) (`file:../../dq-ui/packages/*`).

## Conventions

- Templates: **`Dq*` only**
- Feedback: `toast` / `confirm` from `@/utils/feedback`
- Themes via `@danqing/dq-tokens` (default `mac`)
- Tauri macOS: `installTauriMacosShell()` + `dq-tauri-macos.css`

## Local dev

```bash
cd ../dq-ui && pnpm install
cd ../h3.c.ui && make libh3.dylib
make dev-api          # :7810
cd frontend && npm install && npm run dev   # :5810
```

#!/usr/bin/env bash
# Smoke-test h3-api without requiring model weights.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
API="${ROOT}/crates/target/debug/h3-api"
PORT="${H3_API_PORT:-7819}"

if [[ ! -x "$API" ]]; then
  echo "building h3-api…"
  make -C "$ROOT" api
fi

"$API" --host 127.0.0.1 --port "$PORT" >"/tmp/h3-api-smoke.log" 2>&1 &
PID=$!
cleanup() { kill "$PID" 2>/dev/null || true; }
trap cleanup EXIT

for i in $(seq 1 40); do
  if curl -sf "http://127.0.0.1:${PORT}/api/system/health" >/dev/null; then
    break
  fi
  sleep 0.25
done

echo "== health =="
curl -sf "http://127.0.0.1:${PORT}/api/system/health" | tee /tmp/h3-health.json
echo
echo "== model status =="
curl -sf "http://127.0.0.1:${PORT}/api/model/status" | tee /tmp/h3-model.json
echo
echo "== settings =="
curl -sf "http://127.0.0.1:${PORT}/api/settings" | tee /tmp/h3-settings.json
echo
echo "== generate (expect fail without weights) =="
CODE=$(curl -s -o /tmp/h3-gen.json -w "%{http_code}" \
  -X POST "http://127.0.0.1:${PORT}/api/generate" \
  -H 'content-type: application/json' \
  -d '{"prompt":"smoke","width":512,"height":512,"frames":8,"steps":2,"seed":1,"references":[],"reference_image_size":"match","denoise_reuse":1,"dit_layers":50,"core_reuse":1,"token_reduction":false,"ssd_streaming":false,"use_int8_row_fc2":false,"use_reference_rope":false,"use_slower_bf16_mlp":false,"use_slower_bf16_qkv":false,"use_slower_bf16_attention_output":false,"use_slower_row_major_attention_output":false,"use_slower_unfused_int8_inputs":false,"use_slower_unfused_qkv_rope":false,"use_slower_scalar_qkv_rms":false,"use_slower_uncached_int8_scales":false,"use_slower_dynamic_fc1_k":false,"use_slower_grouped_quantizer":false,"render_width":0,"render_height":0,"preview_denoise":false}')
echo "HTTP $CODE"
cat /tmp/h3-gen.json
echo
TASK_ID=$(python3 -c 'import json;print(json.load(open("/tmp/h3-gen.json"))["task_id"])')
sleep 1
echo "== task =="
curl -sf "http://127.0.0.1:${PORT}/api/tasks/${TASK_ID}" | tee /tmp/h3-task.json
echo
python3 - <<'PY'
import json
h=json.load(open("/tmp/h3-health.json"))
assert h.get("ok") is True
m=json.load(open("/tmp/h3-model.json"))
assert "fl2va_ready" in m
t=json.load(open("/tmp/h3-task.json"))
assert t["status"] in ("failed","cancelled","completed","running","queued")
print("SMOKE OK")
PY

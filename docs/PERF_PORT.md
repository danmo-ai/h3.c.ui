# H3 performance port notes

Measured on Apple M5 Max 128 GiB with `/Users/nil.luo/.h3-ui/models/MiniMax-H3`.

## Stage wall times

Every generation ends with one stderr line (desktop: `~/.h3-ui/api.log`):

```
h3: run shape 864x480 canvas, 243 frames, latent 72x30x54, 29160 video + 400 audio + N text tokens, 8 steps, turbo-lora=1 reuse=1 layers=50
h3: stage wall times (total …s): setup=… text encoder=… DiT prepare=… load transformer core=… denoise=… audio VAE=… video VAE decode=… FFmpeg=…
h3: live denoise previews took …s over 8 steps (included in denoise)
```

Use it before changing knobs: step-count levers (Turbo LoRA, `--reuse`) only
shrink `denoise`; they cannot move load, VAE, or FFmpeg time.

### Duration scaling

Tokens per latent frame are `(W/32)·(H/32)` (VAE /16 then 2×2 patch); latent
frames are `(aligned_frames − 5)/17·5 + 2`. At 864×480:

| Duration | Aligned frames | Video tokens | Linear cost | Attention cost |
|---|---:|---:|---:|---:|
| 2.3 s (default 56f) | 56 | 6 885 | 1× | 1× |
| 10 s | 243 | 29 160 | 4.2× | ~18× |

At 10 s each step is ~2.4 PFLOP over 50 blocks, split roughly evenly between
dense attention and the linears, so only attention-side work
(token reduction, a competitive sparse/flash kernel, lower `render_*` canvas)
and fewer evaluations scale well.

The current Veda kernel is scalar (one query row per threadgroup, keys re-read
from device memory per row, no simdgroup matrix MMA), so a 10 % keep-ratio plan
is not expected to beat MPSGraph dense SDPA yet.

## Module profile (encode-wall shares)

`H3_PROFILE=1` now prints DiT module encode shares at teardown.

| Config | attn encode share | notes |
|---|---:|---|
| 512² / 22f / 4-step / 50-layer (MPSGraph SDPA) | **98.8%** | confirms attention dominates host encode |
| 1344×768 / 22f / 2-step / 35-layer (MPSGraph) | **76.0%** | same conclusion at production width |

Community issue [antirez/h3.c#13](https://github.com/antirez/h3.c/issues/13) reported ~70% GPU module time in attention on PyTorch MPS; native encode shares agree that attention is the long-seq bottleneck.

## Ported paths

| Path | Status | How to enable |
|---|---|---|
| BF16 streamed FlashAttention | wired, opt-in | `H3_FORCE_FLASH_ATTN=1` |
| Online int8 FlashAttention | wired, opt-in | `H3_FORCE_FLASH_ATTN=1 H3_FORCE_INT8_FLASH_ATTN=1` |
| Veda tile-sparse SDPA | wired | `--veda` / `!veda` (plan via local window or `H3_VEDA_PLAN`) |
| Morton linear split >3072 | default | automatic for TensorOps modes |
| Offline int8 MLP weight cache | opt-in | `H3_INT8_CACHE=1` [`H3_INT8_CACHE_DIR=...`] |

## Widescreen A/B (1344×768, 22f, 2 steps, 35 layers)

| Attention path | Denoise wall | root-gpu | dispatches |
|---|---:|---:|---|
| MPSGraph (default) | **13.6 s** | 0.23 s | attention=70 |
| Forced BF16 Flash | 166.9 s | 166.8 s | flash=70 |

Flash is correct (`flash=70`) but not yet competitive; keep opt-in until a TensorOps / fused Flash kernel lands (Draw Things / WWDC26 direction).

## Veda smoke (512², 22f, 2 steps, 35 layers)

```
h3: Veda tile-sparse attention on (15 tiles, 30 pairs, tile=128)
... sparse=70 ...
```

Local keep-ratio 0.10 plan runs end-to-end. Distilled predictor packs can be supplied later through `H3_VEDA_PLAN` (`H3VD` binary CSR).

## Microbench

```sh
make h3_flash_attn_test && ./h3_flash_attn_test
```

Reports BF16 / int8 Flash and sparse timings on a synthetic 4096×4×128 head-major case.

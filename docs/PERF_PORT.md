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

## Correctness audit against reference implementations

Checked against upstream [antirez/h3.c](https://github.com/antirez/h3.c) and
the Turbo LoRA reference sampler
([`larryvrh/MiniMax-H3-Turbo-Lora` `generate.py`](https://huggingface.co/larryvrh/MiniMax-H3-Turbo-Lora)).

| Item | Status |
|---|---|
| Video/audio sigma grids (shift 12 / 3, `1 − i/n`) and dual-clock Euler | matches reference |
| Turbo LoRA coverage (qkv/out/fc1/fc2, token refiner, per-block and final adaLN) | matches reference |
| LoRA scale (`alpha = rank`, no extra scaling) | matches reference |
| LoRA application | **fixed**: was merged into BF16 then int8-quantized; now applied at run time as `y = W·x + B(A·x)` like the reference (`H3_TURBO_LORA_MERGE=1` restores the merge for A/B) |
| `H3_INT8_CACHE` file names | **fixed**: names were `kind_bN`, so FL2VA/Ref2VA and base/Turbo shared cached int8 weights; now prefixed by a checkpoint + LoRA digest (old unprefixed files are ignored) |
| Veda local plan | **fixed**: video tiles only saw a 1-D local window, so most video rows could not attend to text, keyframe conditions, or audio; prefix rows now stay dense as global keys/queries |
| Threadgroup reductions in flash / sparse / GQA-causal / VAE-encoder group-norm kernels | **fixed**: `reductions[0]` was read and then overwritten without a barrier (cross-simdgroup race); the GQA and group-norm instances are inherited from upstream |

### Turbo LoRA merge precision (block 20, real weights)

`‖ΔW‖/‖W‖` is 4e-4 for `attn.out_proj` and 1.5e-4 for `mlp.fc2`. Relative
error of the applied update `x·ΔW_eff` versus the exact `x·(B·A)`:

| Module | Activations | BF16 merge | BF16 merge + int8 (default) |
|---|---|---:|---:|
| `attn.out_proj` | isotropic | 0.79 | 4.30 |
| `attn.out_proj` | inside span(A) | 0.53 | 0.59 |
| `mlp.fc2` | isotropic | 0.96 | 4.42 |
| `mlp.fc2` | inside span(A) | 0.88 | 0.93 |

Even in the best case most of the distilled update is lost, so the merge was
replaced by a run-time rank-64 side path: `+ B·(A·x)` inside the fused int8
QKV epilogue (before Q/K norm and RoPE) and FC1 epilogue (before SwiGLU), and
as a residual add after the head-major attention output and FC2; the adaLN
schedule and token refiner add it after their BF16 linears.
`tests/test_lora_runtime.c` checks the kernels against a CPU reference.

### Turbo LoRA run-time A/B (real weights, step-0 video velocity)

480×288, 22 frames, 8 steps, seed 7, identical inputs. Reference R is the
BF16 path with run-time LoRA (equivalent to `generate.py`); `u = R − base`.

| Config | rel. error vs R | `‖Δ_X‖/‖u‖` | `‖Δ_X − u‖/‖u‖` |
|---|---:|---:|---:|
| int8 + run-time LoRA (default) | **0.059** | 1.02 | 0.14 |
| int8 + merged LoRA (old default) | 0.156 | 0.68 | 0.37 |
| BF16 + merged LoRA | 0.143 | 0.65 | 0.37 |

The LoRA update is 38 % of the velocity norm and the int8 base alone differs
from BF16 by 0.075, so the run-time path sits at the int8 noise floor while
the merge dropped about a third of the distilled update. 10 s 864×480 Turbo
denoise: 515 s run-time vs 614 s merged (run-to-run variance; no measurable
LoRA overhead).

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

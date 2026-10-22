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

`H3_PROFILE=1 H3_PROFILE_SYNC=1` waits for the GPU at each module boundary,
so the shares are GPU time rather than encode wall. 10 s 864×480 (29584
tokens), 2 steps × 50 blocks, M5 Max:

| qkv | attn | attn_out | mlp | other |
|---:|---:|---:|---:|---:|
| 9.3 s (7.7%) | 85.4 s (70.5%) | 3.8 s (3.2%) | 22.4 s (18.4%) | 0.3 s |

Attention costs ~0.85 s per block under sustained load against 0.56 s for
one cold MPSGraph SDPA at the same shape (45 TFLOPS); the gap is thermal
clock reduction, measured on both battery and AC power.

## Visual VAE decoder precision

The ViT decoder (36 blocks, hidden 2048) ran entirely in F32 at ~7.7 TFLOPS.
The reference disables autocast only around `x_embedder` and `proj_out`, so
the transformer body runs with BF16 operands on CUDA. The default now keeps
the residual stream, norms' statistics, embedding and output projection in
F32 and feeds BF16 to the four linears and SDPA (`H3_VAE_F32=1` restores the
F32 path). VAE-shape microbench (2028 rows): linears 19.1 → 4.4 ms per
block, SDPA 4.44 → 0.80 ms.

10 s 864×480 (243 frames, 14 temporal chunks × 8 tiles), real latent:

| Path | decode | peak GPU memory | vs F32 frames |
|---|---:|---:|---|
| F32 | 161 s | 9.4 GiB | — |
| BF16 operands (default) | 33–37 s | 4.8 GiB | PSNR 63.0 dB, worst frame 61.0 dB |

`./h3_video_vae_bench VAE_DIR latent.f32 T H W [out] [reference]` repeats
the comparison; `H3_DUMP_VIDEO_LATENT=path` writes the latent from a run.
The decode was previously reported as "video VAE load" in the stage table.

End-to-end 10 s 864×480 Turbo LoRA (8 steps): 690–794 s before, 657 s after
with the denoise phase thermally limited at ~75 s/step (VAE 36 s).

## Ported paths

| Path | Status | How to enable |
|---|---|---|
| BF16 streamed FlashAttention | wired, opt-in | `H3_FORCE_FLASH_ATTN=1` |
| Online int8 FlashAttention | wired, opt-in | `H3_FORCE_FLASH_ATTN=1 H3_FORCE_INT8_FLASH_ATTN=1` |
| Veda tile-sparse SDPA | TensorOps kernel on M5 | `--veda` / `!veda` / UI "Veda attention" (local window, `H3_VEDA_KEEP_RATIO` default 0.35, or `H3_VEDA_PLAN`) |
| Morton linear split >3072 | default | automatic for TensorOps modes |
| Offline int8 MLP weight cache | opt-in | `H3_INT8_CACHE=1` [`H3_INT8_CACHE_DIR=...`] |

## Widescreen A/B (1344×768, 22f, 2 steps, 35 layers)

| Attention path | Denoise wall | root-gpu | dispatches |
|---|---:|---:|---|
| MPSGraph (default) | **13.6 s** | 0.23 s | attention=70 |
| Forced BF16 Flash | 166.9 s | 166.8 s | flash=70 |

Flash is correct (`flash=70`) but not yet competitive; keep opt-in until a TensorOps / fused Flash kernel lands (Draw Things / WWDC26 direction).

## TensorOps FlashAttention and Veda

`h3_flash_attn_nax_bf16` follows MLX's `attention_nax`: four simdgroups of
16 query rows, 16×32×16 simdgroup matmuls on register fragments, BK = 32,
`max_total_threads_per_threadgroup(128)`, Q reloaded per key block (keeping
Q resident spills: 880–1000 ms), unchecked loads except on the ragged last
block. Dense, cold, S = 29584, 56 heads: 642–672 ms against MPSGraph's
555–580 ms, so dense attention stays on MPSGraph.

The same kernel walks a Veda CSR plan (query tile → key tiles), which
MPSGraph cannot do. The scalar sparse kernel it replaces ran at ~3 TFLOPS
dense-equivalent and made `--veda` slower than dense. Same shape:

| keep ratio | tile pairs | sparse | MPSGraph dense |
|---:|---:|---:|---:|
| 0.25 | 27.8% | 191 ms | 580 ms |
| 0.35 | 37.9% | 263 ms | 578 ms |
| 0.5 | 52.1% | 366 ms | 602 ms |

The local plan keeps text/audio rows global and a window of whole latent
frames for video rows (1620 tokens per frame at 864×480). 10 s 864×480 Turbo
LoRA, 8 steps, seed 7, back to back:

| run | denoise | total |
|---|---:|---:|
| dense | 569 s | 631 s |
| Veda 0.5 | 451 s | 512 s |
| Veda 0.35 (default keep) | 368 s | 429 s |

Sparse runs diverge from the dense sample (PSNR ~12 dB, different motion)
but frames stay sharp and temporally coherent; the default keep ratio moved
from 0.10 to 0.35. `h3_nax_attention_test` checks the dense kernel against
the scalar flash kernel and the sparse path against the scalar sparse kernel.

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

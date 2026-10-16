# H3 performance port notes

Measured on Apple M5 Max 128 GiB with `/Users/nil.luo/.h3-ui/models/MiniMax-H3`.

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

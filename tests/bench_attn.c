#include "h3_gpu.h"
#include "h3_veda.h"

#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

/* Head-major attention timing at DiT shapes:
 *   h3_attn_bench [sequence] [heads] [mode...]
 * modes: mps (MPSGraph SDPA), flash (scalar BF16 flash), nax (TensorOps
 * flash), sparse / sparse-scalar (Veda local plan through the TensorOps or
 * scalar kernel; H3_VEDA_KEEP_RATIO, default 0.3, with the first 424 rows
 * global as in the 10 s layout). Every mode after the first is checked
 * against the first. */

static h3_veda_plan *plan;

static double now(void) {
    struct timespec value;
    clock_gettime(CLOCK_MONOTONIC, &value);
    return (double)value.tv_sec + (double)value.tv_nsec * 1e-9;
}

static uint16_t f32_to_bf16(float value) {
    uint32_t bits;
    memcpy(&bits, &value, sizeof(bits));
    bits += 0x7fffu + ((bits >> 16) & 1u);
    return (uint16_t)(bits >> 16);
}

static float bf16_to_f32(uint16_t value) {
    uint32_t bits = (uint32_t)value << 16;
    float result;
    memcpy(&result, &bits, sizeof(result));
    return result;
}

static int run(h3_gpu *gpu, const char *mode, h3_gpu_tensor *out,
               h3_gpu_tensor *q, h3_gpu_tensor *k, h3_gpu_tensor *v,
               uint32_t sequence, uint32_t heads) {
    float scale = 1.0f / sqrtf(128.0f);
    if (!h3_gpu_begin(gpu)) return 0;
    int ok;
    if (!strcmp(mode, "mps")) {
        h3_gpu_mark_head_major_sdpa_inputs(gpu);
        ok = h3_gpu_sdpa_bf16_head_major_output(gpu, out, q, k, v, sequence,
                                                heads, 128, scale);
    } else if (!strcmp(mode, "flash")) {
        ok = h3_gpu_flash_attn_bf16_head_major(gpu, out, q, k, v, sequence,
                                               heads, 128, scale);
    } else if (!strcmp(mode, "nax")) {
        ok = h3_gpu_flash_attn_nax_bf16_head_major(gpu, out, q, k, v,
                                                   sequence, heads, 128,
                                                   scale);
    } else if (!strcmp(mode, "sparse") || !strcmp(mode, "sparse-scalar")) {
        if (!plan) {
            const char *keep = getenv("H3_VEDA_KEEP_RATIO");
            char error[256];
            plan = h3_veda_plan_build_local(gpu, sequence, 128,
                                            keep ? strtof(keep, NULL) : 0.3f,
                                            424, error, sizeof(error));
            if (!plan) {
                fprintf(stderr, "%s\n", error);
                return 0;
            }
            fprintf(stderr, "Veda plan: %u q tiles, %u pairs (%.1f%%)\n",
                    h3_veda_plan_q_tiles(plan), h3_veda_plan_pairs(plan),
                    100.0 * h3_veda_plan_pairs(plan) /
                    ((double)h3_veda_plan_q_tiles(plan) *
                     h3_veda_plan_q_tiles(plan)));
        }
        if (mode[6]) setenv("H3_DISABLE_NAX_SPARSE_ATTN", "1", 1);
        ok = h3_gpu_sparse_attn_bf16_head_major(
            gpu, out, q, k, v, h3_veda_plan_q_offsets(plan),
            h3_veda_plan_k_tiles(plan), sequence, heads, 128, 128,
            h3_veda_plan_q_tiles(plan), scale);
        unsetenv("H3_DISABLE_NAX_SPARSE_ATTN");
    } else {
        fprintf(stderr, "unknown mode %s\n", mode);
        return 0;
    }
    return ok && h3_gpu_submit(gpu);
}

int main(int argc, char **argv) {
    uint32_t sequence = argc > 1 ? (uint32_t)atoi(argv[1]) : 29584;
    uint32_t heads = argc > 2 ? (uint32_t)atoi(argv[2]) : 56;
    char error[512];
    h3_gpu *gpu = h3_gpu_create("h3_shaders.metal", error, sizeof(error));
    if (!gpu) {
        fprintf(stderr, "%s\n", error);
        return 1;
    }
    size_t count = (size_t)sequence * heads * 128;
    uint16_t *host = malloc(count * sizeof(*host));
    h3_gpu_tensor *qkv[3];
    unsigned state = 1;
    for (int t = 0; t < 3; t++) {
        for (size_t i = 0; i < count; i++) {
            state = state * 1664525u + 1013904223u;
            float u = (float)(state >> 8) / 16777216.0f * 2.0f - 1.0f;
            host[i] = f32_to_bf16(u * (t < 2 ? 1.5f : 1.0f));
        }
        qkv[t] = h3_gpu_tensor_from_bf16(gpu, host, count);
    }
    h3_gpu_tensor *first = h3_gpu_tensor_new_bf16(gpu, count);
    h3_gpu_tensor *out = h3_gpu_tensor_new_bf16(gpu, count);
    uint16_t *a = malloc(count * sizeof(*a));
    uint16_t *b = malloc(count * sizeof(*b));
    double flops = 4.0 * sequence * (double)sequence * 128.0 * heads;
    int modes = argc > 3 ? argc - 3 : 1;
    for (int m = 0; m < modes; m++) {
        const char *mode = argc > 3 ? argv[3 + m] : "mps";
        h3_gpu_tensor *target = m ? out : first;
        if (!run(gpu, mode, target, qkv[0], qkv[1], qkv[2], sequence,
                 heads)) {
            fprintf(stderr, "%s failed: %s\n", mode, h3_gpu_error(gpu));
            return 1;
        }
        const char *iters_env = getenv("H3_BENCH_ITERS");
        int iters = iters_env ? atoi(iters_env) : 3;
        if (iters < 1) iters = 1;
        double best = 1e30, total = 0.0, last = 0.0;
        for (int iteration = 0; iteration < iters; iteration++) {
            double start = now();
            run(gpu, mode, target, qkv[0], qkv[1], qkv[2], sequence, heads);
            last = now() - start;
            total += last;
            if (last < best) best = last;
        }
        printf("%-6s S=%u H=%u: best %8.1f ms (%5.1f TFLOPS) avg %8.1f "
               "last %8.1f ms", mode, sequence, heads, best * 1e3,
               flops / best / 1e12, total / iters * 1e3, last * 1e3);
        if (m) {
            h3_gpu_tensor_read_bf16(first, a, count);
            h3_gpu_tensor_read_bf16(out, b, count);
            double err = 0.0, norm = 0.0;
            for (size_t i = 0; i < count; i++) {
                double d = bf16_to_f32(a[i]) - bf16_to_f32(b[i]);
                err += d * d;
                norm += (double)bf16_to_f32(a[i]) * bf16_to_f32(a[i]);
            }
            printf("  rel-L2 vs %s %.2e", argv[3], sqrt(err / norm));
        }
        printf("\n");
    }
    return 0;
}

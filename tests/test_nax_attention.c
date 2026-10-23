/* TensorOps FlashAttention: the dense kernel must match MPSGraph SDPA and the
 * Veda block-sparse path must match the scalar sparse kernel. */
#include "h3_gpu.h"
#include "h3_veda.h"

#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void die(const char *message, h3_gpu *gpu) {
    fprintf(stderr, "FAIL tests/test_nax_attention.c: %s%s%s\n", message,
            gpu ? ": " : "", gpu ? h3_gpu_error(gpu) : "");
    exit(1);
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

static double compare(const h3_gpu_tensor *a, const h3_gpu_tensor *b,
                      size_t count) {
    uint16_t *x = malloc(count * sizeof(*x));
    uint16_t *y = malloc(count * sizeof(*y));
    if (!x || !y || !h3_gpu_tensor_read_bf16(a, x, count) ||
        !h3_gpu_tensor_read_bf16(b, y, count)) die("readback", NULL);
    double error = 0.0, norm = 0.0;
    for (size_t i = 0; i < count; i++) {
        double delta = (double)bf16_to_f32(x[i]) - bf16_to_f32(y[i]);
        error += delta * delta;
        norm += (double)bf16_to_f32(y[i]) * bf16_to_f32(y[i]);
    }
    free(x);
    free(y);
    return sqrt(error / (norm + 1e-30));
}

int main(void) {
    char error[512];
    h3_gpu *gpu = h3_gpu_create("h3_shaders.metal", error, sizeof(error));
    if (!gpu) {
        fprintf(stderr, "FAIL tests/test_nax_attention.c: %s\n", error);
        return 1;
    }
    if (!h3_gpu_is_m5(gpu)) {
        puts("skip: TensorOps attention needs an M5 GPU");
        return 0;
    }
    /* A ragged length exercises the partial query and key blocks. */
    enum { SEQUENCE = 1000, HEADS = 3, DIM = 128 };
    size_t count = (size_t)SEQUENCE * HEADS * DIM;
    uint16_t *host = malloc(count * sizeof(*host));
    if (!host) die("out of memory", NULL);
    h3_gpu_tensor *qkv[3];
    unsigned state = 11;
    for (int t = 0; t < 3; t++) {
        for (size_t i = 0; i < count; i++) {
            state = state * 1664525u + 1013904223u;
            float u = (float)(state >> 8) / 16777216.0f * 2.0f - 1.0f;
            host[i] = f32_to_bf16(u * (t < 2 ? 3.0f : 1.0f));
        }
        qkv[t] = h3_gpu_tensor_from_bf16(gpu, host, count);
    }
    h3_gpu_tensor *want = h3_gpu_tensor_new_bf16(gpu, count);
    h3_gpu_tensor *got = h3_gpu_tensor_new_bf16(gpu, count);
    float scale = 1.0f / sqrtf((float)DIM);
    if (!want || !got || !h3_gpu_begin(gpu) ||
        !h3_gpu_flash_attn_bf16_head_major(gpu, want, qkv[0], qkv[1], qkv[2],
                                           SEQUENCE, HEADS, DIM, scale) ||
        !h3_gpu_flash_attn_nax_bf16_head_major(gpu, got, qkv[0], qkv[1],
                                               qkv[2], SEQUENCE, HEADS, DIM,
                                               scale) ||
        !h3_gpu_submit(gpu)) die("dense attention", gpu);
    double dense = compare(got, want, count);
    printf("NAX dense vs scalar flash    rel-L2 %.3e\n", dense);
    if (!(dense < 5e-3)) die("dense NAX attention mismatch", NULL);

    h3_veda_plan *plan = h3_veda_plan_build_local(gpu, SEQUENCE, 128, 0.4f,
                                                  200, error, sizeof(error));
    if (!plan) die(error, NULL);
    for (int nax = 0; nax < 2; nax++) {
        if (!nax) setenv("H3_DISABLE_NAX_SPARSE_ATTN", "1", 1);
        else unsetenv("H3_DISABLE_NAX_SPARSE_ATTN");
        if (!h3_gpu_begin(gpu) ||
            !h3_gpu_sparse_attn_bf16_head_major(
                gpu, nax ? got : want, qkv[0], qkv[1], qkv[2],
                h3_veda_plan_q_offsets(plan), h3_veda_plan_k_tiles(plan),
                SEQUENCE, HEADS, DIM, 128, h3_veda_plan_q_tiles(plan),
                scale) ||
            !h3_gpu_submit(gpu)) die("sparse attention", gpu);
    }
    double sparse = compare(got, want, count);
    printf("NAX sparse vs scalar sparse  rel-L2 %.3e\n", sparse);
    if (!(sparse < 5e-3)) die("sparse NAX attention mismatch", NULL);
    puts("ok: TensorOps attention");
    h3_veda_plan_free(plan);
    return 0;
}

#include "h3_gpu.h"
#include "h3_veda.h"

#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

enum { HEADS = 4, HEAD_DIM = 128 };

static double now(void) {
    struct timespec value;
    if (clock_gettime(CLOCK_MONOTONIC, &value) != 0) return 0.0;
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

static void die(const char *message) {
    fprintf(stderr, "FAIL tests/test_flash_attn.c: %s\n", message);
    exit(1);
}

static void fill_head_major(uint16_t *dst, uint32_t sequence, uint32_t heads,
                            uint32_t head_dim, unsigned seed) {
    unsigned state = seed;
    for (uint32_t head = 0; head < heads; head++) {
        for (uint32_t row = 0; row < sequence; row++) {
            for (uint32_t d = 0; d < head_dim; d++) {
                state = state * 1664525u + 1013904223u;
                float value = ((float)(state & 0xffffu) / 65535.0f) * 0.2f -
                              0.1f;
                dst[((size_t)head * sequence + row) * head_dim + d] =
                    f32_to_bf16(value);
            }
        }
    }
}

static int nearly_equal(const uint16_t *a, const uint16_t *b, size_t count,
                        float atol) {
    size_t mismatches = 0;
    float worst = 0.0f;
    for (size_t i = 0; i < count; i++) {
        float diff = fabsf(bf16_to_f32(a[i]) - bf16_to_f32(b[i]));
        if (diff > worst) worst = diff;
        if (diff > atol) mismatches++;
    }
    if (mismatches) {
        fprintf(stderr, "mismatches=%zu/%.1f%% worst=%.5f\n", mismatches,
                100.0 * (double)mismatches / (double)count, worst);
        return 0;
    }
    return 1;
}

int main(void) {
    char error[512];
    h3_gpu *gpu = h3_gpu_create("h3_shaders.metal", error, sizeof(error));
    if (!gpu) die(error);

    const uint32_t short_seq = 256;
    const uint32_t long_seq = 4096;
    size_t short_count = (size_t)short_seq * HEADS * HEAD_DIM;
    size_t long_count = (size_t)long_seq * HEADS * HEAD_DIM;
    uint16_t *q_host = malloc(long_count * sizeof(uint16_t));
    uint16_t *k_host = malloc(long_count * sizeof(uint16_t));
    uint16_t *v_host = malloc(long_count * sizeof(uint16_t));
    uint16_t *out_mps = malloc(long_count * sizeof(uint16_t));
    uint16_t *out_flash = malloc(long_count * sizeof(uint16_t));
    if (!q_host || !k_host || !v_host || !out_mps || !out_flash)
        die("oom");
    fill_head_major(q_host, long_seq, HEADS, HEAD_DIM, 1);
    fill_head_major(k_host, long_seq, HEADS, HEAD_DIM, 2);
    fill_head_major(v_host, long_seq, HEADS, HEAD_DIM, 3);

    h3_gpu_tensor *q = h3_gpu_tensor_from_bf16(gpu, q_host, short_count);
    h3_gpu_tensor *k = h3_gpu_tensor_from_bf16(gpu, k_host, short_count);
    h3_gpu_tensor *v = h3_gpu_tensor_from_bf16(gpu, v_host, short_count);
    h3_gpu_tensor *out_a = h3_gpu_tensor_new_bf16(gpu, short_count);
    h3_gpu_tensor *out_b = h3_gpu_tensor_new_bf16(gpu, short_count);
    if (!q || !k || !v || !out_a || !out_b) die("short tensor alloc");

    setenv("H3_FORCE_FLASH_ATTN", "1", 1);
    setenv("H3_FORCE_BF16_FLASH_ATTN", "1", 1);
    if (!h3_gpu_begin(gpu) ||
        !h3_gpu_flash_attn_bf16_head_major(
            gpu, out_a, q, k, v, short_seq, HEADS, HEAD_DIM,
            1.0f / sqrtf((float)HEAD_DIM)) ||
        !h3_gpu_submit(gpu))
        die(h3_gpu_error(gpu));
    if (!h3_gpu_tensor_read_bf16(out_a, out_flash, short_count))
        die("read flash");

    /* Warm + second BF16 flash run; allow tiny BF16/reduction noise. */
    if (!h3_gpu_begin(gpu) ||
        !h3_gpu_flash_attn_bf16_head_major(
            gpu, out_b, q, k, v, short_seq, HEADS, HEAD_DIM,
            1.0f / sqrtf((float)HEAD_DIM)) ||
        !h3_gpu_submit(gpu) ||
        !h3_gpu_tensor_read_bf16(out_b, out_mps, short_count))
        die("second flash");
    if (!nearly_equal(out_flash, out_mps, short_count, 2e-2f))
        die("flash not stable across runs");

    h3_gpu_tensor_free(q);
    h3_gpu_tensor_free(k);
    h3_gpu_tensor_free(v);
    h3_gpu_tensor_free(out_a);
    h3_gpu_tensor_free(out_b);

    q = h3_gpu_tensor_from_bf16(gpu, q_host, long_count);
    k = h3_gpu_tensor_from_bf16(gpu, k_host, long_count);
    v = h3_gpu_tensor_from_bf16(gpu, v_host, long_count);
    out_a = h3_gpu_tensor_new_bf16(gpu, long_count);
    out_b = h3_gpu_tensor_new_bf16(gpu, long_count);
    if (!q || !k || !v || !out_a || !out_b) die("long tensor alloc");

    unsetenv("H3_FORCE_BF16_FLASH_ATTN");
    double t0 = now();
    if (!h3_gpu_begin(gpu) ||
        !h3_gpu_flash_attn_int8_head_major(
            gpu, out_a, q, k, v, long_seq, HEADS, HEAD_DIM,
            1.0f / sqrtf((float)HEAD_DIM)) ||
        !h3_gpu_submit(gpu))
        die(h3_gpu_error(gpu));
    double int8_s = now() - t0;

    t0 = now();
    if (!h3_gpu_begin(gpu) ||
        !h3_gpu_flash_attn_bf16_head_major(
            gpu, out_b, q, k, v, long_seq, HEADS, HEAD_DIM,
            1.0f / sqrtf((float)HEAD_DIM)) ||
        !h3_gpu_submit(gpu))
        die(h3_gpu_error(gpu));
    double bf16_s = now() - t0;

    h3_veda_plan *plan = h3_veda_plan_build_local(gpu, long_seq, 128, 0.10f,
                                                  error, sizeof(error));
    if (!plan) die(error);
    t0 = now();
    if (!h3_gpu_begin(gpu) ||
        !h3_gpu_sparse_attn_bf16_head_major(
            gpu, out_a, q, k, v, h3_veda_plan_q_offsets(plan),
            h3_veda_plan_k_tiles(plan), long_seq, HEADS, HEAD_DIM,
            h3_veda_plan_tile_size(plan), h3_veda_plan_q_tiles(plan),
            1.0f / sqrtf((float)HEAD_DIM)) ||
        !h3_gpu_submit(gpu))
        die(h3_gpu_error(gpu));
    double sparse_s = now() - t0;

    h3_gpu_stats stats;
    h3_gpu_get_stats(gpu, &stats);
    printf("flash_attn ok: seq=%u heads=%u int8=%.3fs bf16=%.3fs "
           "sparse=%.3fs flash_dispatches=%llu sparse_dispatches=%llu "
           "pairs=%u\n",
           long_seq, HEADS, int8_s, bf16_s, sparse_s,
           (unsigned long long)stats.flash_attn_dispatches,
           (unsigned long long)stats.sparse_attn_dispatches,
           h3_veda_plan_pairs(plan));

    h3_veda_plan_free(plan);
    h3_gpu_tensor_free(q);
    h3_gpu_tensor_free(k);
    h3_gpu_tensor_free(v);
    h3_gpu_tensor_free(out_a);
    h3_gpu_tensor_free(out_b);
    free(q_host);
    free(k_host);
    free(v_host);
    free(out_mps);
    free(out_flash);
    h3_gpu_free(gpu);
    return 0;
}

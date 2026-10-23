/* The mixed-precision visual VAE kernels must agree with their F32
 * counterparts up to BF16 rounding of the operands and results. */
#include "h3_gpu.h"

#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void die(const char *message, h3_gpu *gpu) {
    fprintf(stderr, "FAIL tests/test_video_vae_mixed.c: %s%s%s\n", message,
            gpu ? ": " : "", gpu ? h3_gpu_error(gpu) : "");
    exit(1);
}

static float bf16_round(float value) {
    uint32_t bits;
    memcpy(&bits, &value, sizeof(bits));
    bits += 0x7fffu + ((bits >> 16) & 1u);
    bits &= 0xffff0000u;
    memcpy(&value, &bits, sizeof(value));
    return value;
}

static void fill(float *values, size_t count, unsigned *state, float scale,
                 int round) {
    for (size_t i = 0; i < count; i++) {
        *state = *state * 1664525u + 1013904223u;
        float u = ((float)(*state >> 8) / 16777216.0f * 2.0f - 1.0f) * scale;
        values[i] = round ? bf16_round(u) : u;
    }
}

static double rel_l2(const float *got, const float *want, size_t count) {
    double error = 0.0, norm = 0.0;
    for (size_t i = 0; i < count; i++) {
        double delta = (double)got[i] - want[i];
        error += delta * delta;
        norm += (double)want[i] * want[i];
    }
    return sqrt(error / (norm + 1e-30));
}

static void check(const char *name, const float *got, const float *want,
                  size_t count, double limit) {
    double error = rel_l2(got, want, count);
    printf("%-28s rel-L2 %.3e\n", name, error);
    if (!(error < limit)) die(name, NULL);
}

static h3_gpu_tensor *bf16_from(h3_gpu *gpu, const float *values,
                                size_t count) {
    h3_gpu_tensor *f32 = h3_gpu_tensor_from_f32(gpu, values, count);
    h3_gpu_tensor *bf16 = h3_gpu_tensor_new_bf16(gpu, count);
    if (!f32 || !bf16 || !h3_gpu_begin(gpu) ||
        !h3_gpu_cast_f32_to_bf16(gpu, bf16, f32, (uint32_t)count) ||
        !h3_gpu_submit(gpu)) die("BF16 upload", gpu);
    h3_gpu_tensor_free(f32);
    return bf16;
}

static void read_bf16(h3_gpu *gpu, const h3_gpu_tensor *tensor, float *values,
                      size_t count) {
    h3_gpu_tensor *f32 = h3_gpu_tensor_new_f32(gpu, count);
    if (!f32 || !h3_gpu_begin(gpu) ||
        !h3_gpu_cast_bf16_to_f32(gpu, f32, tensor, (uint32_t)count) ||
        !h3_gpu_submit(gpu) || !h3_gpu_tensor_read_f32(f32, values, count))
        die("BF16 readback", gpu);
    h3_gpu_tensor_free(f32);
}

int main(void) {
    char error[512];
    h3_gpu *gpu = h3_gpu_create("h3_shaders.metal", error, sizeof(error));
    if (!gpu) {
        fprintf(stderr, "FAIL tests/test_video_vae_mixed.c: %s\n", error);
        return 1;
    }
    enum { ROWS = 77, HEADS = 4, DIM = 64, HALF = 24, WIDTH = 2048 };
    unsigned state = 7;
    size_t inner = (size_t)HEADS * DIM;
    size_t qkv_count = (size_t)ROWS * inner * 3;
    size_t out_count = (size_t)ROWS * inner;
    size_t rope_count = (size_t)ROWS * HALF;
    float *qkv = malloc(qkv_count * sizeof(*qkv));
    float *cosines = malloc(rope_count * sizeof(*cosines));
    float *sines = malloc(rope_count * sizeof(*sines));
    float *want = malloc(3 * out_count * sizeof(*want));
    float *got = malloc(3 * out_count * sizeof(*got));
    if (!qkv || !cosines || !sines || !want || !got) die("out of memory", NULL);
    fill(qkv, qkv_count, &state, 3.0f, 1);
    for (size_t i = 0; i < rope_count; i++) {
        float angle = (float)i * 0.37f;
        cosines[i] = cosf(angle);
        sines[i] = sinf(angle);
    }
    h3_gpu_tensor *qkv_f32 = h3_gpu_tensor_from_f32(gpu, qkv, qkv_count);
    h3_gpu_tensor *qkv_bf16 = bf16_from(gpu, qkv, qkv_count);
    h3_gpu_tensor *rope_cos = h3_gpu_tensor_from_f32(gpu, cosines, rope_count);
    h3_gpu_tensor *rope_sin = h3_gpu_tensor_from_f32(gpu, sines, rope_count);
    h3_gpu_tensor *f32_out[3], *bf16_out[3];
    for (int i = 0; i < 3; i++) {
        f32_out[i] = h3_gpu_tensor_new_f32(gpu, out_count);
        bf16_out[i] = h3_gpu_tensor_new_bf16(gpu, out_count);
    }
    if (!h3_gpu_begin(gpu) ||
        !h3_gpu_video_qkv_rope_f32(gpu, f32_out[0], f32_out[1], f32_out[2],
            qkv_f32, rope_cos, rope_sin, ROWS, HEADS, DIM, HALF, 1e-5f) ||
        !h3_gpu_video_qkv_rope_bf16(gpu, bf16_out[0], bf16_out[1], bf16_out[2],
            qkv_bf16, rope_cos, rope_sin, ROWS, HEADS, DIM, HALF, 1e-5f) ||
        !h3_gpu_submit(gpu)) die("video QKV/RoPE", gpu);
    static const char *names[] = {"video QKV RoPE query",
                                  "video QKV RoPE key", "video QKV value"};
    for (int i = 0; i < 3; i++) {
        if (!h3_gpu_tensor_read_f32(f32_out[i], want, out_count))
            die("F32 readback", gpu);
        read_bf16(gpu, bf16_out[i], got, out_count);
        check(names[i], got, want, out_count, 6e-3);
    }

    size_t count = (size_t)ROWS * WIDTH;
    float *input = malloc(count * sizeof(*input));
    float *branch = malloc(count * sizeof(*branch));
    float *weight = malloc(WIDTH * sizeof(*weight));
    float *norm_want = malloc(count * sizeof(*norm_want));
    float *norm_got = malloc(count * sizeof(*norm_got));
    if (!input || !branch || !weight || !norm_want || !norm_got)
        die("out of memory", NULL);
    fill(input, count, &state, 4.0f, 0);
    fill(branch, count, &state, 2.0f, 1);
    fill(weight, WIDTH, &state, 1.5f, 0);
    h3_gpu_tensor *x = h3_gpu_tensor_from_f32(gpu, input, count);
    h3_gpu_tensor *w = h3_gpu_tensor_from_f32(gpu, weight, WIDTH);
    h3_gpu_tensor *b_f32 = h3_gpu_tensor_from_f32(gpu, branch, count);
    h3_gpu_tensor *b_bf16 = bf16_from(gpu, branch, count);
    h3_gpu_tensor *norm_f32 = h3_gpu_tensor_new_f32(gpu, count);
    h3_gpu_tensor *norm_bf16 = h3_gpu_tensor_new_bf16(gpu, count);
    h3_gpu_tensor *sum_f32 = h3_gpu_tensor_new_f32(gpu, count);
    h3_gpu_tensor *sum_mixed = h3_gpu_tensor_new_f32(gpu, count);
    if (!h3_gpu_begin(gpu) ||
        !h3_gpu_rms_norm_f32(gpu, norm_f32, x, w, ROWS, WIDTH, 1e-5f) ||
        !h3_gpu_rms_norm_f32_to_bf16(gpu, norm_bf16, x, w, ROWS, WIDTH,
                                     1e-5f) ||
        !h3_gpu_scale_add_f32(gpu, sum_f32, x, b_f32, w, ROWS, WIDTH) ||
        !h3_gpu_scale_add_bf16_branch_f32(gpu, sum_mixed, x, b_bf16, w,
                                          ROWS, WIDTH) ||
        !h3_gpu_submit(gpu)) die("RMSNorm/scale-add", gpu);
    if (!h3_gpu_tensor_read_f32(norm_f32, norm_want, count))
        die("F32 readback", gpu);
    read_bf16(gpu, norm_bf16, norm_got, count);
    check("RMSNorm F32->BF16", norm_got, norm_want, count, 4e-3);
    if (!h3_gpu_tensor_read_f32(sum_f32, norm_want, count) ||
        !h3_gpu_tensor_read_f32(sum_mixed, norm_got, count))
        die("F32 readback", gpu);
    check("scale-add BF16 branch", norm_got, norm_want, count, 1e-6);
    puts("ok: video VAE mixed-precision kernels");
    return 0;
}

#include "h3_gpu.h"

#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int failures;

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

static float round_bf16(float value) { return bf16_to_f32(f32_to_bf16(value)); }

static unsigned rng_state = 12345u;

static float uniform(void) {
    rng_state = rng_state * 1664525u + 1013904223u;
    return (float)(rng_state >> 8) / 16777216.0f * 2.0f - 1.0f;
}

static uint16_t *random_bf16(size_t count, float scale) {
    uint16_t *values = malloc(count * sizeof(*values));
    if (!values) exit(1);
    for (size_t index = 0; index < count; index++)
        values[index] = f32_to_bf16(uniform() * scale);
    return values;
}

static void check(int condition, const char *label) {
    if (!condition) {
        fprintf(stderr, "FAIL %s\n", label);
        failures++;
    }
}

static void compare(const char *label, const float *got, const float *want,
                    size_t count, float tolerance) {
    double error = 0.0, norm = 0.0;
    float worst = 0.0f;
    for (size_t index = 0; index < count; index++) {
        double diff = (double)got[index] - (double)want[index];
        error += diff * diff;
        norm += (double)want[index] * want[index];
        if (fabsf((float)diff) > worst) worst = fabsf((float)diff);
    }
    double relative = norm > 0.0 ? sqrt(error / norm) : sqrt(error);
    printf("%-44s rel-L2 %.3e  max-abs %.3e\n", label, relative, worst);
    check(relative <= tolerance, label);
}

static float *read_bf16(const h3_gpu_tensor *tensor, size_t count) {
    uint16_t *raw = malloc(count * sizeof(*raw));
    float *values = malloc(count * sizeof(*values));
    if (!raw || !values || !h3_gpu_tensor_read_bf16(tensor, raw, count)) {
        fprintf(stderr, "cannot read tensor\n");
        exit(1);
    }
    for (size_t index = 0; index < count; index++)
        values[index] = bf16_to_f32(raw[index]);
    free(raw);
    return values;
}

static void test_lora_product(h3_gpu *gpu, uint32_t rows, uint32_t input_dim,
                              uint32_t rank, uint32_t output_dim) {
    uint16_t *x = random_bf16((size_t)rows * input_dim, 1.0f);
    uint16_t *a = random_bf16((size_t)rank * input_dim, 0.02f);
    uint16_t *b = random_bf16((size_t)output_dim * rank, 0.05f);
    h3_gpu_tensor *tx = h3_gpu_tensor_from_bf16(gpu, x,
                                                (size_t)rows * input_dim);
    h3_gpu_tensor *ta = h3_gpu_tensor_from_bf16(gpu, a,
                                                (size_t)rank * input_dim);
    h3_gpu_tensor *tb = h3_gpu_tensor_from_bf16(gpu, b,
                                                (size_t)output_dim * rank);
    h3_gpu_tensor *scratch = h3_gpu_tensor_new_bf16(gpu, (size_t)rows * rank);
    h3_gpu_tensor *delta = h3_gpu_tensor_new_bf16(
        gpu, (size_t)rows * output_dim);
    check(h3_gpu_begin(gpu) &&
          h3_gpu_lora_bf16(gpu, delta, scratch, tx, ta, tb, rows, input_dim,
                           rank, output_dim) &&
          h3_gpu_submit(gpu), h3_gpu_error(gpu));
    float *got = read_bf16(delta, (size_t)rows * output_dim);
    float *want = malloc((size_t)rows * output_dim * sizeof(*want));
    float *hidden = malloc((size_t)rank * sizeof(*hidden));
    for (uint32_t row = 0; row < rows; row++) {
        for (uint32_t r = 0; r < rank; r++) {
            float sum = 0.0f;
            for (uint32_t k = 0; k < input_dim; k++)
                sum += bf16_to_f32(x[(size_t)row * input_dim + k]) *
                       bf16_to_f32(a[(size_t)r * input_dim + k]);
            hidden[r] = round_bf16(sum);
        }
        for (uint32_t column = 0; column < output_dim; column++) {
            float sum = 0.0f;
            for (uint32_t r = 0; r < rank; r++)
                sum += hidden[r] * bf16_to_f32(b[(size_t)column * rank + r]);
            want[(size_t)row * output_dim + column] = round_bf16(sum);
        }
    }
    char label[96];
    snprintf(label, sizeof(label), "LoRA product %ux%u r%u -> %u", rows,
             input_dim, rank, output_dim);
    compare(label, got, want, (size_t)rows * output_dim, 1e-2f);
    free(x); free(a); free(b); free(got); free(want); free(hidden);
    h3_gpu_tensor_free(tx); h3_gpu_tensor_free(ta); h3_gpu_tensor_free(tb);
    h3_gpu_tensor_free(scratch); h3_gpu_tensor_free(delta);
}

static void test_head_major_rows(h3_gpu *gpu) {
    const uint32_t rows = 37, heads = 3, head_dim = 128;
    size_t count = (size_t)rows * heads * head_dim;
    uint16_t *input = random_bf16(count, 1.0f);
    h3_gpu_tensor *tin = h3_gpu_tensor_from_bf16(gpu, input, count);
    h3_gpu_tensor *tout = h3_gpu_tensor_new_bf16(gpu, count);
    check(h3_gpu_begin(gpu) &&
          h3_gpu_head_major_to_rows_bf16(gpu, tout, tin, rows, heads,
                                         head_dim) &&
          h3_gpu_submit(gpu), h3_gpu_error(gpu));
    float *got = read_bf16(tout, count);
    float *want = malloc(count * sizeof(*want));
    for (uint32_t row = 0; row < rows; row++)
        for (uint32_t head = 0; head < heads; head++)
            for (uint32_t d = 0; d < head_dim; d++)
                want[((size_t)row * heads + head) * head_dim + d] =
                    bf16_to_f32(input[((size_t)head * rows + row) *
                                      head_dim + d]);
    compare("head-major to rows", got, want, count, 0.0f);
    free(input); free(got); free(want);
    h3_gpu_tensor_free(tin); h3_gpu_tensor_free(tout);
}

/* Zero int8 weights isolate the delta: V must equal the delta, Q/K the
 * per-head RMS norm of it (identity RoPE, unit norm weights). */
static void test_qkv_delta(h3_gpu *gpu, uint32_t rows, uint32_t input_dim,
                           int unfused) {
    const uint32_t heads = 4, head_dim = 128, rope_half = 48;
    uint32_t inner = heads * head_dim;
    uint32_t padded = (rows + 127u) & ~127u;
    uint16_t *input = random_bf16((size_t)rows * input_dim, 1.0f);
    uint16_t *delta = random_bf16((size_t)rows * inner * 3, 1.0f);
    int8_t *weight = calloc((size_t)inner * 3 * input_dim, 1);
    float *ones = malloc((size_t)inner * 3 * sizeof(*ones));
    for (uint32_t index = 0; index < inner * 3; index++) ones[index] = 1.0f;
    uint16_t norm[128], *cosine = malloc((size_t)rows * rope_half * 2),
             *sine = calloc((size_t)rows * rope_half, 2);
    for (uint32_t index = 0; index < 128; index++)
        norm[index] = f32_to_bf16(1.0f);
    for (size_t index = 0; index < (size_t)rows * rope_half; index++)
        cosine[index] = f32_to_bf16(1.0f);
    h3_gpu_tensor *tin = h3_gpu_tensor_from_bf16(gpu, input,
                                                 (size_t)rows * input_dim);
    h3_gpu_tensor *tdelta = h3_gpu_tensor_from_bf16(
        gpu, delta, (size_t)rows * inner * 3);
    h3_gpu_tensor *tw = h3_gpu_tensor_from_i8(gpu, weight,
                                              (size_t)inner * 3 * input_dim);
    h3_gpu_tensor *tscale = h3_gpu_tensor_from_f32(gpu, ones, inner * 3);
    h3_gpu_tensor *tnorm = h3_gpu_tensor_from_bf16(gpu, norm, 128);
    h3_gpu_tensor *tcos = h3_gpu_tensor_from_bf16(
        gpu, cosine, (size_t)rows * rope_half);
    h3_gpu_tensor *tsin = h3_gpu_tensor_from_bf16(
        gpu, sine, (size_t)rows * rope_half);
    h3_gpu_tensor *quant = h3_gpu_tensor_new_i8(gpu,
                                                (size_t)padded * input_dim);
    h3_gpu_tensor *qscale = h3_gpu_tensor_new_f32(gpu, padded);
    h3_gpu_tensor *q = h3_gpu_tensor_new_bf16(gpu, (size_t)rows * inner);
    h3_gpu_tensor *k = h3_gpu_tensor_new_bf16(gpu, (size_t)rows * inner);
    h3_gpu_tensor *v = h3_gpu_tensor_new_bf16(gpu, (size_t)rows * inner);
    check(h3_gpu_begin(gpu) &&
          h3_gpu_grouped_qkv_linear_rope_int8_lora(
              gpu, q, k, v, quant, qscale, tin, tw, tscale, tnorm, tnorm,
              tcos, tsin, tdelta, rows, input_dim, heads, head_dim,
              rope_half, 1e-5f, unfused, 0, 0) &&
          h3_gpu_submit(gpu), h3_gpu_error(gpu));
    size_t count = (size_t)rows * inner;
    float *got_q = read_bf16(q, count), *got_k = read_bf16(k, count);
    float *got_v = read_bf16(v, count);
    float *want_q = malloc(count * sizeof(float));
    float *want_k = malloc(count * sizeof(float));
    float *want_v = malloc(count * sizeof(float));
    for (uint32_t row = 0; row < rows; row++) {
        for (uint32_t head = 0; head < heads; head++) {
            size_t out = ((size_t)head * rows + row) * head_dim;
            const uint16_t *source = delta + (size_t)row * inner * 3 +
                                     (size_t)head * head_dim * 3;
            for (uint32_t stream = 0; stream < 2; stream++) {
                float sum = 0.0f;
                for (uint32_t d = 0; d < head_dim; d++) {
                    float value = bf16_to_f32(source[stream * head_dim + d]);
                    sum += value * value;
                }
                float inverse = 1.0f / sqrtf(sum / head_dim + 1e-5f);
                float *want = stream ? want_k : want_q;
                for (uint32_t d = 0; d < head_dim; d++)
                    want[out + d] =
                        bf16_to_f32(source[stream * head_dim + d]) * inverse;
            }
            for (uint32_t d = 0; d < head_dim; d++)
                want_v[out + d] = bf16_to_f32(source[2 * head_dim + d]);
        }
    }
    char label[96];
    snprintf(label, sizeof(label), "QKV delta V rows=%u k=%u%s", rows,
             input_dim, unfused ? " unfused" : "");
    compare(label, got_v, want_v, count, 0.0f);
    snprintf(label, sizeof(label), "QKV delta Q rows=%u k=%u%s", rows,
             input_dim, unfused ? " unfused" : "");
    compare(label, got_q, want_q, count, 1e-2f);
    snprintf(label, sizeof(label), "QKV delta K rows=%u k=%u%s", rows,
             input_dim, unfused ? " unfused" : "");
    compare(label, got_k, want_k, count, 1e-2f);
    free(input); free(delta); free(weight); free(ones); free(cosine);
    free(sine); free(got_q); free(got_k); free(got_v); free(want_q);
    free(want_k); free(want_v);
    h3_gpu_tensor *all[] = {tin, tdelta, tw, tscale, tnorm, tcos, tsin,
                            quant, qscale, q, k, v};
    for (size_t index = 0; index < sizeof(all) / sizeof(*all); index++)
        h3_gpu_tensor_free(all[index]);
}

/* Zero FC1/FC2 weights: activated must equal SiLU(delta_gate) * delta_up. */
static void test_fc1_delta(h3_gpu *gpu, uint32_t rows, uint32_t input_dim) {
    const uint32_t hidden = 1024, output = 128;
    uint32_t padded = (rows + 127u) & ~127u;
    uint16_t *input = random_bf16((size_t)rows * input_dim, 1.0f);
    uint16_t *delta = random_bf16((size_t)rows * hidden * 2, 2.0f);
    int8_t *fc1 = calloc((size_t)hidden * 2 * input_dim, 1);
    int8_t *fc2 = calloc((size_t)output * hidden, 1);
    float *ones = malloc((size_t)hidden * 2 * sizeof(*ones));
    for (uint32_t index = 0; index < hidden * 2; index++) ones[index] = 1.0f;
    size_t capacity = (size_t)padded * (input_dim > hidden ?
                                        input_dim : hidden);
    h3_gpu_tensor *tin = h3_gpu_tensor_from_bf16(gpu, input,
                                                 (size_t)rows * input_dim);
    h3_gpu_tensor *tdelta = h3_gpu_tensor_from_bf16(
        gpu, delta, (size_t)rows * hidden * 2);
    h3_gpu_tensor *tfc1 = h3_gpu_tensor_from_i8(
        gpu, fc1, (size_t)hidden * 2 * input_dim);
    h3_gpu_tensor *tfc2 = h3_gpu_tensor_from_i8(gpu, fc2,
                                                (size_t)output * hidden);
    h3_gpu_tensor *s1 = h3_gpu_tensor_from_f32(gpu, ones, hidden * 2);
    h3_gpu_tensor *s2 = h3_gpu_tensor_from_f32(gpu, ones, output);
    h3_gpu_tensor *quant = h3_gpu_tensor_new_i8(gpu, capacity);
    h3_gpu_tensor *qscale = h3_gpu_tensor_new_f32(gpu, padded);
    h3_gpu_tensor *activated = h3_gpu_tensor_new_bf16(
        gpu, (size_t)rows * hidden);
    h3_gpu_tensor *out = h3_gpu_tensor_new_bf16(gpu, (size_t)rows * output);
    check(h3_gpu_begin(gpu) &&
          h3_gpu_mlp_int8_lora_bf16(gpu, out, activated, quant, qscale, tin,
                                    tfc1, s1, tfc2, s2, tdelta, rows,
                                    input_dim, hidden, output, 0, 0, 0) &&
          h3_gpu_submit(gpu), h3_gpu_error(gpu));
    size_t count = (size_t)rows * hidden;
    float *got = read_bf16(activated, count);
    float *want = malloc(count * sizeof(*want));
    for (uint32_t row = 0; row < rows; row++)
        for (uint32_t column = 0; column < hidden; column++) {
            float gate = bf16_to_f32(delta[(size_t)row * hidden * 2 + column]);
            float up = bf16_to_f32(
                delta[(size_t)row * hidden * 2 + hidden + column]);
            want[(size_t)row * hidden + column] =
                gate / (1.0f + expf(-gate)) * up;
        }
    char label[96];
    snprintf(label, sizeof(label), "FC1 delta SwiGLU rows=%u k=%u", rows,
             input_dim);
    compare(label, got, want, count, 1e-2f);
    free(input); free(delta); free(fc1); free(fc2); free(ones); free(got);
    free(want);
    h3_gpu_tensor *all[] = {tin, tdelta, tfc1, tfc2, s1, s2, quant, qscale,
                            activated, out};
    for (size_t index = 0; index < sizeof(all) / sizeof(*all); index++)
        h3_gpu_tensor_free(all[index]);
}

int main(void) {
    char error[512];
    h3_gpu *gpu = h3_gpu_create("h3_shaders.metal", error, sizeof(error));
    if (!gpu) {
        fprintf(stderr, "cannot create GPU: %s\n", error);
        return 1;
    }
    test_head_major_rows(gpu);
    test_lora_product(gpu, 256, 5376, 64, 1536);
    test_lora_product(gpu, 2176, 512, 64, 256);
    test_lora_product(gpu, 7, 512, 16, 96);
    if (h3_gpu_has_int8_mlp(gpu)) {
        test_qkv_delta(gpu, 256, 256, 0);
        test_qkv_delta(gpu, 200, 5376, 0);
        test_qkv_delta(gpu, 2176, 5376, 0);
        test_qkv_delta(gpu, 256, 256, 1);
        test_fc1_delta(gpu, 256, 256);
        test_fc1_delta(gpu, 200, 5376);
    } else {
        printf("int8 TensorOps unavailable; skipped fused delta tests\n");
    }
    h3_gpu_free(gpu);
    if (failures) {
        fprintf(stderr, "%d LoRA runtime checks failed\n", failures);
        return 1;
    }
    printf("ok: LoRA runtime\n");
    return 0;
}

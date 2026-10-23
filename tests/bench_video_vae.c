/* Decode a raw F32 [24, T, H, W] latent with the visual VAE, report the wall
 * time, optionally write the RGB frames, and compare against a previous run.
 *
 *   ./h3_video_vae_bench MODEL/FL2VA/video_vae/source latent.f32 T H W \
 *       [out.rgbf32] [reference.rgbf32]
 */
#include "h3_video_vae.h"

#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

static double now(void) {
    struct timespec value;
    clock_gettime(CLOCK_MONOTONIC, &value);
    return (double)value.tv_sec + (double)value.tv_nsec * 1e-9;
}

static float *read_floats(const char *path, size_t count) {
    FILE *file = fopen(path, "rb");
    float *values = malloc(count * sizeof(*values));
    if (!file || !values || fread(values, sizeof(*values), count, file) != count) {
        fprintf(stderr, "cannot read %zu floats from %s\n", count, path);
        if (file) fclose(file);
        free(values);
        return NULL;
    }
    fclose(file);
    return values;
}

int main(int argc, char **argv) {
    if (argc < 6) {
        fprintf(stderr, "usage: %s VAE_DIR LATENT T H W [OUT] [REFERENCE]\n",
                argv[0]);
        return 2;
    }
    int t = atoi(argv[3]), h = atoi(argv[4]), w = atoi(argv[5]);
    size_t latent_count = (size_t)24 * t * h * w;
    float *latent = read_floats(argv[2], latent_count);
    if (!latent) return 1;
    char error[512];
    h3_video_frames frames;
    double start = now();
    if (!h3_video_vae_decode(argv[1], "h3_shaders.metal", latent, t, h, w,
                             NULL, NULL, &frames, error, sizeof(error))) {
        fprintf(stderr, "decode failed: %s\n", error);
        return 1;
    }
    double elapsed = now() - start;
    size_t count = (size_t)frames.frames * frames.height * frames.width * 3;
    printf("decoded %d frames %dx%d in %.1f s\n", frames.frames, frames.width,
           frames.height, elapsed);
    if (argc > 6) {
        FILE *file = fopen(argv[6], "wb");
        if (!file || fwrite(frames.rgb, sizeof(float), count, file) != count) {
            fprintf(stderr, "cannot write %s\n", argv[6]);
            return 1;
        }
        fclose(file);
    }
    if (argc > 7) {
        float *reference = read_floats(argv[7], count);
        if (!reference) return 1;
        double square = 0.0, maximum = 0.0, worst_frame = 1e30;
        size_t per_frame = count / (size_t)frames.frames;
        for (int frame = 0; frame < frames.frames; frame++) {
            double frame_square = 0.0;
            for (size_t i = 0; i < per_frame; i++) {
                size_t index = (size_t)frame * per_frame + i;
                double delta = (double)frames.rgb[index] - reference[index];
                frame_square += delta * delta;
                if (fabs(delta) > maximum) maximum = fabs(delta);
            }
            square += frame_square;
            double psnr = 10.0 * log10(
                1.0 / (frame_square / (double)per_frame + 1e-20));
            if (psnr < worst_frame) worst_frame = psnr;
        }
        double psnr = 10.0 * log10(1.0 / (square / (double)count + 1e-20));
        printf("vs reference: PSNR %.2f dB (worst frame %.2f dB), "
               "max abs %.4f (%.1f/255)\n",
               psnr, worst_frame, maximum, maximum * 255.0);
        free(reference);
    }
    h3_video_frames_free(&frames);
    free(latent);
    return 0;
}

#include "h3_veda.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

struct h3_veda_plan {
    uint32_t sequence;
    uint32_t tile_size;
    uint32_t q_tiles;
    uint32_t pair_count;
    uint32_t *host_q_offsets;
    uint32_t *host_k_tiles;
    h3_gpu_tensor *q_offsets;
    h3_gpu_tensor *k_tiles;
};

static void fail(char *error, size_t error_size, const char *message) {
    if (error && error_size) {
        snprintf(error, error_size, "%s", message);
    }
}

void h3_veda_plan_free(h3_veda_plan *plan) {
    if (!plan) return;
    h3_gpu_tensor_free(plan->q_offsets);
    h3_gpu_tensor_free(plan->k_tiles);
    free(plan->host_q_offsets);
    free(plan->host_k_tiles);
    free(plan);
}

uint32_t h3_veda_plan_tile_size(const h3_veda_plan *plan) {
    return plan ? plan->tile_size : 0;
}

uint32_t h3_veda_plan_q_tiles(const h3_veda_plan *plan) {
    return plan ? plan->q_tiles : 0;
}

uint32_t h3_veda_plan_pairs(const h3_veda_plan *plan) {
    return plan ? plan->pair_count : 0;
}

const h3_gpu_tensor *h3_veda_plan_q_offsets(const h3_veda_plan *plan) {
    return plan ? plan->q_offsets : NULL;
}

const h3_gpu_tensor *h3_veda_plan_k_tiles(const h3_veda_plan *plan) {
    return plan ? plan->k_tiles : NULL;
}

static h3_veda_plan *plan_from_host(h3_gpu *gpu, uint32_t sequence,
                                    uint32_t tile_size, uint32_t q_tiles,
                                    const uint32_t *q_offsets,
                                    const uint32_t *k_tiles,
                                    uint32_t pair_count, char *error,
                                    size_t error_size) {
    h3_veda_plan *plan = calloc(1, sizeof(*plan));
    if (!plan) {
        fail(error, error_size, "out of memory for Veda plan");
        return NULL;
    }
    plan->sequence = sequence;
    plan->tile_size = tile_size;
    plan->q_tiles = q_tiles;
    plan->pair_count = pair_count;
    plan->host_q_offsets = malloc(sizeof(uint32_t) * ((size_t)q_tiles + 1u));
    plan->host_k_tiles = pair_count ?
        malloc(sizeof(uint32_t) * pair_count) : NULL;
    if (!plan->host_q_offsets || (pair_count && !plan->host_k_tiles)) {
        fail(error, error_size, "out of memory copying Veda plan");
        h3_veda_plan_free(plan);
        return NULL;
    }
    memcpy(plan->host_q_offsets, q_offsets,
           sizeof(uint32_t) * ((size_t)q_tiles + 1u));
    if (pair_count)
        memcpy(plan->host_k_tiles, k_tiles, sizeof(uint32_t) * pair_count);
    plan->q_offsets = h3_gpu_tensor_from_u32(gpu, q_offsets, q_tiles + 1);
    plan->k_tiles = pair_count ?
        h3_gpu_tensor_from_u32(gpu, k_tiles, pair_count) :
        h3_gpu_tensor_from_u32(gpu, q_offsets, 1);
    if (!plan->q_offsets || !plan->k_tiles) {
        fail(error, error_size, "cannot upload Veda plan tensors");
        h3_veda_plan_free(plan);
        return NULL;
    }
    return plan;
}

static int local_pair_selected(uint32_t q, uint32_t k, uint32_t q_tiles,
                               uint32_t keep, uint32_t global_tiles) {
    if (q < global_tiles || k < global_tiles) return 1;
    /* Always keep the diagonal tile, then expand a symmetric window. */
    int32_t radius = (int32_t)((keep - 1u) / 2u);
    int32_t start = (int32_t)q - radius;
    if (start < 0) start = 0;
    uint32_t end = (uint32_t)start + keep;
    if (end > q_tiles) {
        end = q_tiles;
        start = (int32_t)(end - keep);
        if (start < 0) start = 0;
    }
    return k >= (uint32_t)start && k < end;
}

int h3_veda_local_pairs(uint32_t sequence, uint32_t tile_size,
                        float keep_ratio, uint32_t global_rows,
                        uint32_t **q_offsets_out, uint32_t **k_tiles_out,
                        uint32_t *q_tiles_out, uint32_t *pair_count_out) {
    if (!q_offsets_out || !k_tiles_out || !q_tiles_out || !pair_count_out ||
        !sequence || !tile_size || keep_ratio <= 0.0f || keep_ratio > 1.0f ||
        global_rows > sequence) return 0;
    *q_offsets_out = NULL;
    *k_tiles_out = NULL;
    uint32_t q_tiles = (sequence + tile_size - 1u) / tile_size;
    uint32_t keep = (uint32_t)((float)q_tiles * keep_ratio + 0.999f);
    if (keep < 1u) keep = 1u;
    if (keep > q_tiles) keep = q_tiles;
    uint32_t global_tiles = (global_rows + tile_size - 1u) / tile_size;

    size_t pair_count = 0;
    for (uint32_t q = 0; q < q_tiles; q++)
        for (uint32_t k = 0; k < q_tiles; k++)
            pair_count += (size_t)local_pair_selected(q, k, q_tiles, keep,
                                                      global_tiles);
    if (pair_count > UINT32_MAX) return 0;
    uint32_t *q_offsets = calloc((size_t)q_tiles + 1u, sizeof(uint32_t));
    uint32_t *k_tiles = calloc(pair_count ? pair_count : 1u,
                               sizeof(uint32_t));
    if (!q_offsets || !k_tiles) {
        free(q_offsets);
        free(k_tiles);
        return 0;
    }
    uint32_t cursor = 0;
    for (uint32_t q = 0; q < q_tiles; q++) {
        q_offsets[q] = cursor;
        for (uint32_t k = 0; k < q_tiles; k++)
            if (local_pair_selected(q, k, q_tiles, keep, global_tiles))
                k_tiles[cursor++] = k;
    }
    q_offsets[q_tiles] = cursor;
    *q_offsets_out = q_offsets;
    *k_tiles_out = k_tiles;
    *q_tiles_out = q_tiles;
    *pair_count_out = cursor;
    return 1;
}

h3_veda_plan *h3_veda_plan_build_local(h3_gpu *gpu, uint32_t sequence,
                                       uint32_t tile_size, float keep_ratio,
                                       uint32_t global_rows,
                                       char *error, size_t error_size) {
    if (!gpu || !sequence || !tile_size || keep_ratio <= 0.0f ||
        keep_ratio > 1.0f || global_rows > sequence) {
        fail(error, error_size, "invalid Veda local-plan arguments");
        return NULL;
    }
    uint32_t *q_offsets = NULL, *k_tiles = NULL;
    uint32_t q_tiles = 0, pair_count = 0;
    if (!h3_veda_local_pairs(sequence, tile_size, keep_ratio, global_rows,
                             &q_offsets, &k_tiles, &q_tiles, &pair_count)) {
        fail(error, error_size, "out of memory building Veda local plan");
        return NULL;
    }

    h3_veda_plan *plan = plan_from_host(gpu, sequence, tile_size, q_tiles,
                                        q_offsets, k_tiles, pair_count, error,
                                        error_size);
    free(q_offsets);
    free(k_tiles);
    return plan;
}

h3_veda_plan *h3_veda_plan_load(h3_gpu *gpu, const char *path,
                                uint32_t sequence, char *error,
                                size_t error_size) {
    if (!gpu || !path || !*path || !sequence) {
        fail(error, error_size, "invalid Veda plan load arguments");
        return NULL;
    }
    FILE *file = fopen(path, "rb");
    if (!file) {
        fail(error, error_size, "cannot open Veda plan file");
        return NULL;
    }
    char magic[4];
    uint32_t tile_size = 0, q_tiles = 0, pair_count = 0;
    int ok = fread(magic, 1, 4, file) == 4 &&
             memcmp(magic, "H3VD", 4) == 0 &&
             fread(&tile_size, sizeof(tile_size), 1, file) == 1 &&
             fread(&q_tiles, sizeof(q_tiles), 1, file) == 1 &&
             fread(&pair_count, sizeof(pair_count), 1, file) == 1;
    if (!ok || !tile_size || !q_tiles) {
        fclose(file);
        fail(error, error_size, "invalid Veda plan header");
        return NULL;
    }
    uint32_t expected_tiles = (sequence + tile_size - 1u) / tile_size;
    if (q_tiles != expected_tiles) {
        fclose(file);
        fail(error, error_size, "Veda plan tile count does not match sequence");
        return NULL;
    }
    uint32_t *q_offsets = calloc((size_t)q_tiles + 1u, sizeof(uint32_t));
    uint32_t *k_tiles = pair_count ? calloc(pair_count, sizeof(uint32_t)) : NULL;
    ok = q_offsets &&
         (!pair_count || k_tiles) &&
         fread(q_offsets, sizeof(uint32_t), (size_t)q_tiles + 1u, file) ==
             (size_t)q_tiles + 1u &&
         (!pair_count ||
          fread(k_tiles, sizeof(uint32_t), pair_count, file) == pair_count) &&
         q_offsets[q_tiles] == pair_count;
    fclose(file);
    if (!ok) {
        free(q_offsets);
        free(k_tiles);
        fail(error, error_size, "truncated or corrupt Veda plan body");
        return NULL;
    }
    h3_veda_plan *plan = plan_from_host(gpu, sequence, tile_size, q_tiles,
                                        q_offsets, k_tiles, pair_count, error,
                                        error_size);
    free(q_offsets);
    free(k_tiles);
    return plan;
}

int h3_veda_plan_save(const h3_veda_plan *plan, const char *path,
                      char *error, size_t error_size) {
    if (!plan || !path || !*path || !plan->host_q_offsets) {
        fail(error, error_size, "invalid Veda plan save arguments");
        return 0;
    }
    FILE *file = fopen(path, "wb");
    if (!file) {
        fail(error, error_size, "cannot create Veda plan file");
        return 0;
    }
    const char magic[4] = {'H', '3', 'V', 'D'};
    int ok = fwrite(magic, 1, 4, file) == 4 &&
             fwrite(&plan->tile_size, sizeof(plan->tile_size), 1, file) == 1 &&
             fwrite(&plan->q_tiles, sizeof(plan->q_tiles), 1, file) == 1 &&
             fwrite(&plan->pair_count, sizeof(plan->pair_count), 1, file) == 1 &&
             fwrite(plan->host_q_offsets, sizeof(uint32_t),
                    (size_t)plan->q_tiles + 1u, file) ==
                 (size_t)plan->q_tiles + 1u &&
             (!plan->pair_count ||
              fwrite(plan->host_k_tiles, sizeof(uint32_t), plan->pair_count,
                     file) == plan->pair_count);
    fclose(file);
    if (!ok) {
        fail(error, error_size, "failed writing Veda plan file");
        return 0;
    }
    return 1;
}

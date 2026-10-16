#ifndef H3_VEDA_H
#define H3_VEDA_H

#include "h3_gpu.h"

#include <stddef.h>
#include <stdint.h>

typedef struct h3_veda_plan h3_veda_plan;

/* Build a local-window + diagonal tile plan (no predictor). keep_ratio in
 * (0,1] selects approximately that fraction of key tiles per query tile. */
h3_veda_plan *h3_veda_plan_build_local(h3_gpu *gpu, uint32_t sequence,
                                       uint32_t tile_size, float keep_ratio,
                                       char *error, size_t error_size);

/* Load a binary plan written by h3_veda_plan_save / Miowtion export.
 * Magic "H3VD", then tile_size, q_tiles, pair_count, q_offsets[q_tiles+1],
 * k_tiles[pair_count] as little-endian u32. */
h3_veda_plan *h3_veda_plan_load(h3_gpu *gpu, const char *path,
                                uint32_t sequence, char *error,
                                size_t error_size);

int h3_veda_plan_save(const h3_veda_plan *plan, const char *path,
                      char *error, size_t error_size);

void h3_veda_plan_free(h3_veda_plan *plan);

uint32_t h3_veda_plan_tile_size(const h3_veda_plan *plan);
uint32_t h3_veda_plan_q_tiles(const h3_veda_plan *plan);
uint32_t h3_veda_plan_pairs(const h3_veda_plan *plan);
const h3_gpu_tensor *h3_veda_plan_q_offsets(const h3_veda_plan *plan);
const h3_gpu_tensor *h3_veda_plan_k_tiles(const h3_veda_plan *plan);

#endif

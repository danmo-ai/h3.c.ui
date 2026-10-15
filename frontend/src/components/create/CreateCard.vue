<script setup lang="ts">
import { computed, ref } from 'vue';
import { api } from '@/utils/api';

const props = defineProps<{
  name: string;
  bytes?: number;
  modified?: number | null;
}>();

const emit = defineEmits<{
  remove: [];
  open: [];
}>();

const videoRef = ref<HTMLVideoElement | null>(null);

const src = computed(() => {
  const base = api.outputFileUrl(props.name);
  const withQuery = props.modified ? `${base}?v=${props.modified}` : base;
  return `${withQuery}#t=0.1`;
});

function showPosterFrame() {
  const v = videoRef.value;
  if (!v) return;
  // Seek past t=0 so Safari/WebKit paints a visible frame for the card.
  if (Number.isFinite(v.duration) && v.duration > 0) {
    const t = Math.min(0.1, Math.max(0.001, v.duration * 0.02));
    if (Math.abs(v.currentTime - t) > 0.001) {
      try {
        v.currentTime = t;
      } catch {
        /* ignore seek failures on incomplete media */
      }
    }
  }
}
</script>

<template>
  <article class="h3-studio-card">
    <button type="button" class="h3-studio-card__media" @click="emit('open')">
      <video
        ref="videoRef"
        :src="src"
        muted
        preload="auto"
        playsinline
        @loadedmetadata="showPosterFrame"
        @loadeddata="showPosterFrame"
        @seeked="showPosterFrame"
      />
      <span class="h3-studio-card__play" aria-hidden="true">▶</span>
    </button>
    <div class="h3-studio-card__footer">
      <div class="h3-studio-card__name" :title="name">{{ name }}</div>
      <DqButton size="sm" variant="ghost" @click="emit('remove')">Delete</DqButton>
    </div>
  </article>
</template>

<style scoped>
.h3-studio-card {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
}

.h3-studio-card__media {
  position: relative;
  aspect-ratio: 1 / 1;
  overflow: hidden;
  border-radius: var(--dq-radius-group, 12px);
  border: 0.5px solid var(--dq-glass-border, transparent);
  background: #000;
  padding: 0;
  cursor: zoom-in;
  color: inherit;
}

.h3-studio-card__media video {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
  pointer-events: none;
  background: #111;
}

.h3-studio-card__play {
  position: absolute;
  inset: auto 10px 10px auto;
  width: 28px;
  height: 28px;
  border-radius: 999px;
  display: grid;
  place-items: center;
  font-size: 11px;
  background: color-mix(in srgb, #000 55%, transparent);
  color: #fff;
  opacity: 0.85;
}

.h3-studio-card__media:hover .h3-studio-card__play {
  opacity: 1;
}

.h3-studio-card__footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.h3-studio-card__name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: var(--dq-font-size-caption, 12px);
  opacity: 0.72;
}
</style>

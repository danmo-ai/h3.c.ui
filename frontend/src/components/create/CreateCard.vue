<script setup lang="ts">
import { api } from '@/utils/api';

defineProps<{
  name: string;
  bytes?: number;
}>();

const emit = defineEmits<{ remove: [] }>();
</script>

<template>
  <article class="h3-studio-card">
    <div class="h3-studio-card__media">
      <video :src="api.outputFileUrl(name)" controls preload="metadata" />
    </div>
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
}

.h3-studio-card__media video {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
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

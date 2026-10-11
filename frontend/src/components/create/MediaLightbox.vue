<script setup lang="ts">
import { onMounted, onUnmounted, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { api } from '@/utils/api';

const props = defineProps<{
  open: boolean;
  name: string | null;
}>();

const emit = defineEmits<{
  'update:open': [boolean];
}>();

const { t } = useI18n();

function close() {
  emit('update:open', false);
}

function onKey(ev: KeyboardEvent) {
  if (ev.key === 'Escape') close();
}

watch(
  () => props.open,
  (v) => {
    document.documentElement.classList.toggle('h3-lightbox-open', v);
  },
);

onMounted(() => window.addEventListener('keydown', onKey));
onUnmounted(() => {
  window.removeEventListener('keydown', onKey);
  document.documentElement.classList.remove('h3-lightbox-open');
});
</script>

<template>
  <Teleport to="body">
    <div
      v-if="open && name"
      class="h3-lightbox"
      role="dialog"
      aria-modal="true"
      @click.self="close"
    >
      <div class="h3-lightbox__panel">
        <header class="h3-lightbox__head">
          <div class="h3-lightbox__title" :title="name">{{ name }}</div>
          <DqButton size="sm" variant="ghost" @click="close">{{ t('create.close') }}</DqButton>
        </header>
        <video
          class="h3-lightbox__video"
          :src="api.outputFileUrl(name)"
          controls
          autoplay
          playsinline
        />
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.h3-lightbox {
  position: fixed;
  inset: 0;
  z-index: 80;
  display: grid;
  place-items: center;
  padding: 24px;
  background: color-mix(in srgb, #000 72%, transparent);
  backdrop-filter: blur(10px);
}

.h3-lightbox__panel {
  width: min(960px, 100%);
  max-height: min(90vh, 900px);
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-width: 0;
}

.h3-lightbox__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  color: #fff;
}

.h3-lightbox__title {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: var(--dq-font-size-body, 14px);
  font-weight: 600;
}

.h3-lightbox__video {
  width: 100%;
  max-height: calc(90vh - 56px);
  border-radius: 12px;
  background: #000;
  outline: none;
}
</style>

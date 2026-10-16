<script setup lang="ts">
import { computed, onMounted, onUnmounted, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { api } from '@/utils/api';
import { toast } from '@/utils/feedback';

const props = defineProps<{
  open: boolean;
  name: string | null;
  names?: string[];
}>();

const emit = defineEmits<{
  'update:open': [boolean];
  'update:name': [string | null];
}>();

const { t } = useI18n();

const list = computed(() => props.names ?? (props.name ? [props.name] : []));

const index = computed(() => {
  if (!props.name) return -1;
  return list.value.indexOf(props.name);
});

const hasPrev = computed(() => index.value > 0);
const hasNext = computed(() => index.value >= 0 && index.value < list.value.length - 1);

const counter = computed(() => {
  if (index.value < 0 || list.value.length === 0) return '';
  return `${index.value + 1} / ${list.value.length}`;
});

const videoSrc = computed(() => (props.name ? api.outputFileUrl(props.name) : ''));

function close() {
  emit('update:open', false);
}

function goPrev() {
  if (!hasPrev.value) return;
  emit('update:name', list.value[index.value - 1]!);
}

function goNext() {
  if (!hasNext.value) return;
  emit('update:name', list.value[index.value + 1]!);
}

function triggerAnchorDownload(href: string, fileName: string) {
  const a = document.createElement('a');
  a.href = href;
  a.download = fileName;
  a.rel = 'noopener';
  document.body.appendChild(a);
  a.click();
  a.remove();
}

async function downloadCurrent() {
  if (!props.name) return;
  const fileName = props.name;
  try {
    const res = await fetch(api.outputFileUrl(fileName));
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const blob = await res.blob();
    const url = URL.createObjectURL(blob);
    triggerAnchorDownload(url, fileName);
    URL.revokeObjectURL(url);
  } catch (e) {
    try {
      triggerAnchorDownload(api.outputFileUrl(fileName), fileName);
    } catch {
      toast.error(String((e as Error)?.message || e).slice(0, 120));
    }
  }
}

function onKey(ev: KeyboardEvent) {
  if (!props.open) return;
  if (ev.key === 'Escape') {
    close();
    return;
  }
  if (ev.key === 'ArrowLeft') {
    ev.preventDefault();
    goPrev();
    return;
  }
  if (ev.key === 'ArrowRight') {
    ev.preventDefault();
    goNext();
  }
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
      <button
        type="button"
        class="h3-lightbox__nav h3-lightbox__nav--prev"
        :disabled="!hasPrev"
        :aria-label="t('create.prev')"
        @click="goPrev"
      >
        ‹
      </button>

      <div class="h3-lightbox__panel">
        <header class="h3-lightbox__head">
          <div class="h3-lightbox__meta">
            <div class="h3-lightbox__title" :title="name">{{ name }}</div>
            <div v-if="counter" class="h3-lightbox__counter">{{ counter }}</div>
          </div>
          <div class="h3-lightbox__actions">
            <DqButton size="sm" variant="ghost" @click="downloadCurrent">
              {{ t('create.saveVideo') }}
            </DqButton>
            <DqButton size="sm" variant="ghost" @click="close">{{ t('create.close') }}</DqButton>
          </div>
        </header>
        <video
          :key="name"
          class="h3-lightbox__video"
          :src="videoSrc"
          controls
          autoplay
          playsinline
          preload="metadata"
        />
      </div>

      <button
        type="button"
        class="h3-lightbox__nav h3-lightbox__nav--next"
        :disabled="!hasNext"
        :aria-label="t('create.next')"
        @click="goNext"
      >
        ›
      </button>
    </div>
  </Teleport>
</template>

<style scoped>
.h3-lightbox {
  position: fixed;
  inset: 0;
  z-index: 80;
  display: grid;
  grid-template-columns: 56px minmax(0, 1fr) 56px;
  align-items: center;
  gap: 8px;
  padding: 20px 12px;
  background: color-mix(in srgb, #000 78%, transparent);
  backdrop-filter: blur(12px);
}

.h3-lightbox__panel {
  width: min(1280px, 100%);
  max-height: min(94vh, 1080px);
  justify-self: center;
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

.h3-lightbox__meta {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.h3-lightbox__title {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: var(--dq-font-size-body, 14px);
  font-weight: 600;
}

.h3-lightbox__counter {
  font-size: 12px;
  opacity: 0.65;
}

.h3-lightbox__actions {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}

.h3-lightbox__video {
  width: 100%;
  max-height: calc(94vh - 64px);
  border-radius: 14px;
  background: #000;
  outline: none;
  box-shadow: 0 24px 80px color-mix(in srgb, #000 55%, transparent);
}

.h3-lightbox__nav {
  width: 48px;
  height: 48px;
  border: 0;
  border-radius: 999px;
  justify-self: center;
  display: grid;
  place-items: center;
  font-size: 32px;
  line-height: 1;
  color: #fff;
  background: color-mix(in srgb, #fff 14%, transparent);
  cursor: pointer;
  transition: background 0.15s ease, opacity 0.15s ease;
}

.h3-lightbox__nav:hover:not(:disabled) {
  background: color-mix(in srgb, #fff 24%, transparent);
}

.h3-lightbox__nav:disabled {
  opacity: 0.28;
  cursor: default;
}

@media (max-width: 720px) {
  .h3-lightbox {
    grid-template-columns: 1fr;
    grid-template-rows: auto 1fr auto;
    padding: 12px;
  }

  .h3-lightbox__nav {
    width: 100%;
    height: 40px;
    border-radius: 10px;
    font-size: 26px;
  }

  .h3-lightbox__nav--prev {
    order: 2;
  }

  .h3-lightbox__panel {
    order: 1;
  }

  .h3-lightbox__nav--next {
    order: 3;
  }
}
</style>

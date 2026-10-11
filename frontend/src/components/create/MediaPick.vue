<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { api } from '@/utils/api';
import { toast } from '@/utils/feedback';

const props = withDefaults(
  defineProps<{
    modelValue: string;
    accept?: string;
    variant?: 'image' | 'file';
    label?: string;
  }>(),
  {
    accept: 'image/png,image/jpeg,image/webp,image/bmp,image/gif',
    variant: 'image',
  },
);

const emit = defineEmits<{
  'update:modelValue': [string];
  uploaded: [{ name: string; path: string; original_name: string; kind: string }];
}>();

const { t } = useI18n();
const inputRef = ref<HTMLInputElement | null>(null);
const uploading = ref(false);
const uploadName = ref('');
const displayName = ref('');
const localPreview = ref('');

const previewSrc = computed(() => {
  if (localPreview.value) return localPreview.value;
  if (uploadName.value) return api.uploadFileUrl(uploadName.value);
  return '';
});

const hasFile = computed(() => Boolean(props.modelValue));

watch(
  () => props.modelValue,
  (path) => {
    if (!path) {
      uploadName.value = '';
      displayName.value = '';
      clearLocalPreview();
      return;
    }
    const base = path.split(/[/\\]/).pop() || path;
    if (!displayName.value) displayName.value = base;
    // Recover preview for files already stored under uploads/
    if (!uploadName.value && /^[0-9a-f]{32}_.+\.[a-z0-9]+$/i.test(base)) {
      uploadName.value = base;
    }
  },
  { immediate: true },
);

onBeforeUnmount(() => clearLocalPreview());

function clearLocalPreview() {
  if (localPreview.value) {
    URL.revokeObjectURL(localPreview.value);
    localPreview.value = '';
  }
}

function openPicker() {
  if (uploading.value) return;
  inputRef.value?.click();
}

async function onFileChange(ev: Event) {
  const input = ev.target as HTMLInputElement;
  const file = input.files?.[0];
  input.value = '';
  if (!file) return;

  clearLocalPreview();
  if (props.variant === 'image' && file.type.startsWith('image/')) {
    localPreview.value = URL.createObjectURL(file);
  }
  displayName.value = file.name;
  uploading.value = true;
  try {
    const res = await api.uploadFile(file);
    uploadName.value = res.name;
    emit('update:modelValue', res.path);
    emit('uploaded', {
      name: res.name,
      path: res.path,
      original_name: res.original_name,
      kind: res.kind,
    });
  } catch (e) {
    clearLocalPreview();
    displayName.value = '';
    uploadName.value = '';
    emit('update:modelValue', '');
    toast.error(String((e as Error).message || e));
  } finally {
    uploading.value = false;
  }
}

function clear() {
  clearLocalPreview();
  uploadName.value = '';
  displayName.value = '';
  emit('update:modelValue', '');
}
</script>

<template>
  <div class="h3-media-pick" :class="[`is-${variant}`, { 'has-file': hasFile, 'is-uploading': uploading }]">
    <input
      ref="inputRef"
      type="file"
      class="h3-media-pick__input"
      :accept="accept"
      @change="onFileChange"
    />

    <button
      v-if="variant === 'image'"
      type="button"
      class="h3-media-pick__tile"
      :disabled="uploading"
      @click="openPicker"
    >
      <img v-if="previewSrc" :src="previewSrc" alt="" class="h3-media-pick__thumb" />
      <div v-else class="h3-media-pick__placeholder">
        <span class="h3-media-pick__plus">+</span>
        <span>{{ label || t('create.pickImage') }}</span>
      </div>
      <div v-if="uploading" class="h3-media-pick__busy">{{ t('create.uploading') }}</div>
    </button>

    <div v-else class="h3-media-pick__row">
      <button type="button" class="h3-media-pick__file-btn" :disabled="uploading" @click="openPicker">
        <span v-if="uploading">{{ t('create.uploading') }}</span>
        <span v-else-if="hasFile" class="h3-media-pick__filename" :title="displayName">
          {{ displayName || t('create.pickFile') }}
        </span>
        <span v-else>{{ label || t('create.pickFile') }}</span>
      </button>
      <DqButton v-if="hasFile && !uploading" size="sm" variant="ghost" @click="clear">
        {{ t('create.clear') }}
      </DqButton>
    </div>

    <div v-if="variant === 'image' && hasFile && !uploading" class="h3-media-pick__actions">
      <DqButton size="sm" variant="ghost" @click="openPicker">{{ t('create.replace') }}</DqButton>
      <DqButton size="sm" variant="ghost" @click="clear">{{ t('create.clear') }}</DqButton>
    </div>
  </div>
</template>

<style scoped>
.h3-media-pick {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
}

.h3-media-pick__input {
  display: none;
}

.h3-media-pick__tile {
  position: relative;
  display: block;
  width: 100%;
  aspect-ratio: 16 / 10;
  border-radius: 10px;
  border: 1px dashed color-mix(in srgb, var(--dq-label-primary, #fff) 22%, transparent);
  background: color-mix(in srgb, var(--dq-label-primary, #fff) 4%, transparent);
  padding: 0;
  overflow: hidden;
  cursor: pointer;
  color: inherit;
  font: inherit;
}

.h3-media-pick__tile:hover:not(:disabled) {
  border-color: color-mix(in srgb, var(--dq-label-primary, #fff) 40%, transparent);
  background: color-mix(in srgb, var(--dq-label-primary, #fff) 7%, transparent);
}

.h3-media-pick__tile:disabled {
  cursor: wait;
  opacity: 0.85;
}

.h3-media-pick__thumb {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}

.h3-media-pick__placeholder {
  width: 100%;
  height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 4px;
  font-size: var(--dq-font-size-caption, 12px);
  opacity: 0.55;
}

.h3-media-pick__plus {
  font-size: 22px;
  line-height: 1;
  opacity: 0.8;
}

.h3-media-pick__busy {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  background: color-mix(in srgb, var(--dq-bg-primary, #000) 55%, transparent);
  font-size: var(--dq-font-size-caption, 12px);
}

.h3-media-pick__row {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}

.h3-media-pick__file-btn {
  flex: 1 1 auto;
  min-width: 0;
  text-align: left;
  border-radius: 8px;
  border: 1px solid color-mix(in srgb, var(--dq-label-primary, #fff) 14%, transparent);
  background: color-mix(in srgb, var(--dq-label-primary, #fff) 4%, transparent);
  padding: 8px 10px;
  font: inherit;
  font-size: var(--dq-font-size-caption, 12px);
  color: inherit;
  cursor: pointer;
}

.h3-media-pick__file-btn:hover:not(:disabled) {
  border-color: color-mix(in srgb, var(--dq-label-primary, #fff) 30%, transparent);
}

.h3-media-pick__file-btn:disabled {
  cursor: wait;
  opacity: 0.75;
}

.h3-media-pick__filename {
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.h3-media-pick__actions {
  display: flex;
  gap: 4px;
  justify-content: flex-end;
}
</style>

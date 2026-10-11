<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { api } from '@/utils/api';
import { toast } from '@/utils/feedback';
import { THEME_OPTIONS, useThemeStore } from '@/stores/theme';
import { setLocale, type LocaleId } from '@/utils/i18n';

const { t, locale } = useI18n();
const themeStore = useThemeStore();

const modelDir = ref('');
const outputDir = ref('');
const cacheEnabled = ref(true);
const hfRepo = ref('MiniMaxAI/MiniMax-H3');
const ffmpeg = ref(false);
const ffprobe = ref(false);
const mediaSource = ref('');
const ffmpegPath = ref('');

onMounted(async () => {
  const s = await api.getSettings();
  modelDir.value = s.model_dir;
  outputDir.value = s.output_dir;
  cacheEnabled.value = s.cache_enabled;
  hfRepo.value = s.hf_repo;
  const f = await api.ffmpeg();
  ffmpeg.value = f.ffmpeg;
  ffprobe.value = f.ffprobe;
  mediaSource.value = f.source || '';
  ffmpegPath.value = f.ffmpeg_path || '';
});

async function save() {
  try {
    await api.putSettings({
      model_dir: modelDir.value,
      output_dir: outputDir.value,
      cache_enabled: cacheEnabled.value,
      hf_repo: hfRepo.value,
    });
    toast.success('Saved');
  } catch (e) {
    toast.error(String((e as Error).message || e));
  }
}

function onLocale(v: string) {
  setLocale(v as LocaleId);
}
</script>

<template>
  <div class="h3-page">
    <h1>{{ t('settings.title') }}</h1>

    <div class="h3-settings">
      <DqPrefRow :label="t('settings.theme')">
        <DqSelect
          size="sm"
          :model-value="themeStore.currentTheme"
          @update:model-value="(v: string) => themeStore.setTheme(v as any)"
        >
          <DqOption
            v-for="opt in THEME_OPTIONS"
            :key="opt.id"
            :value="opt.id"
            :label="opt.label"
          />
        </DqSelect>
      </DqPrefRow>
      <DqPrefRow :label="t('settings.language')">
        <DqSelect size="sm" :model-value="locale" @update:model-value="onLocale">
          <DqOption value="zh" label="中文" />
          <DqOption value="en" label="English" />
        </DqSelect>
      </DqPrefRow>
      <DqPrefRow :label="t('settings.modelDir')" stacked>
        <DqInput v-model="modelDir" size="sm" />
      </DqPrefRow>
      <DqPrefRow :label="t('settings.outputDir')" stacked>
        <DqInput v-model="outputDir" size="sm" />
      </DqPrefRow>
      <DqPrefRow :label="t('settings.hfRepo')" stacked>
        <DqInput v-model="hfRepo" size="sm" />
      </DqPrefRow>
      <DqPrefRow :label="t('settings.cache')">
        <DqSwitch v-model="cacheEnabled" size="sm" />
      </DqPrefRow>
      <p class="h3-muted">
        {{ t('settings.ffmpeg') }}: {{ ffmpeg ? 'ok' : 'missing' }}
        · {{ t('settings.ffprobe') }}: {{ ffprobe ? 'ok' : 'missing' }}
        <template v-if="mediaSource"> · {{ mediaSource }}</template>
      </p>
      <p v-if="ffmpegPath" class="h3-muted h3-path">{{ ffmpegPath }}</p>
      <div>
        <DqButton size="sm" type="primary" @click="save">{{ t('settings.save') }}</DqButton>
      </div>
    </div>
  </div>
</template>

<style scoped>
.h3-settings {
  max-width: 520px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.h3-settings :deep(.dq-pref-row) {
  min-height: 40px;
  padding: 8px 0;
  border-bottom: 0.5px solid color-mix(in srgb, var(--dq-label-primary, #fff) 7%, transparent);
}

.h3-settings :deep(.dq-pref-row__content) {
  display: flex;
  justify-content: flex-end;
  min-width: 0;
}

.h3-path {
  font-size: var(--dq-font-size-caption, 12px);
  word-break: break-all;
  opacity: 0.55;
}
</style>

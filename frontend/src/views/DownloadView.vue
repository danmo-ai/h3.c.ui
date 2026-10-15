<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
import { storeToRefs } from 'pinia';
import { useI18n } from 'vue-i18n';
import { api, type DownloadPackageId, type DownloadSourceId, type ModelStatus } from '@/utils/api';
import { useDownloadStore } from '@/stores/download';

const { t } = useI18n();
const download = useDownloadStore();
const { progress, busy, source, activePackage, terminalEpoch } = storeToRefs(download);
const status = ref<ModelStatus | null>(null);

const sourceOptions = computed(() => [
  { value: 'modelscope', label: t('download.sourceModelscope') },
  { value: 'hf_mirror', label: t('download.sourceHfMirror') },
  { value: 'huggingface', label: t('download.sourceHuggingFace') },
]);

const activeRepo = computed(() => {
  if (!status.value) return '…';
  if (source.value === 'modelscope') return status.value.modelscope_repo || 'MiniMax/MiniMax-H3';
  return status.value.hf_repo || 'MiniMaxAI/MiniMax-H3';
});

const sourceHint = computed(() => {
  switch (source.value) {
    case 'modelscope':
      return t('download.hintModelscope');
    case 'hf_mirror':
      return t('download.hintHfMirror');
    default:
      return t('download.hintHuggingFace');
  }
});

const progressPct = computed(() => Math.round(progress.value?.progress || 0));

const progressLine = computed(() => {
  const p = progress.value;
  if (!p) return '';
  const parts: string[] = [];
  if (p.package) parts.push(p.package);
  if (p.files_total) parts.push(`${p.files_done ?? 0}/${p.files_total}`);
  if (p.current_file) parts.push(p.current_file);
  else if (p.message) parts.push(p.message);
  if (p.downloaded_bytes != null) {
    parts.push(fmtBytes(p.downloaded_bytes));
    if (p.total_bytes) parts.push(`/ ${fmtBytes(p.total_bytes)}`);
  }
  if (p.speed_bps && p.speed_bps > 1) {
    parts.push(`${(p.speed_bps / 1024 / 1024).toFixed(1)} MiB/s`);
  }
  return parts.join(' · ');
});

function fmtBytes(n: number) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KiB`;
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MiB`;
  return `${(n / 1024 ** 3).toFixed(2)} GiB`;
}

async function refresh() {
  status.value = await api.modelStatus();
  const s = status.value.download_source;
  if (s === 'huggingface' || s === 'hf_mirror' || s === 'modelscope') {
    download.setSource(s);
  }
}

async function start(pkg: DownloadPackageId) {
  await download.start(pkg);
}

watch(terminalEpoch, () => {
  void refresh();
});

onMounted(async () => {
  await refresh();
  await download.restore();
});
</script>

<template>
  <div class="h3-download">
    <header class="h3-download__head">
      <h1>{{ t('download.title') }}</h1>
      <p class="h3-muted">{{ t('download.subtitle') }}</p>
    </header>

    <section class="h3-download__source">
      <div class="h3-download__section-label">{{ t('download.source') }}</div>
      <DqSegmented
        size="sm"
        :model-value="source"
        :options="sourceOptions"
        :disabled="busy"
        @update:model-value="(v: string) => download.setSource(v as DownloadSourceId)"
      />
      <p class="h3-muted">{{ sourceHint }}</p>
      <p class="h3-muted">{{ activeRepo }} · {{ fmtBytes(status?.disk_bytes || 0) }}</p>
    </section>

    <section class="h3-download__packages">
      <div class="h3-download__section-label">{{ t('download.packages') }}</div>

      <div class="h3-download__pkg" data-state="fl2va">
        <div class="h3-download__pkg-head">
          <div>
            <div class="h3-download__pkg-title">{{ t('download.pkgFl2va') }}</div>
            <p class="h3-muted">{{ t('download.pkgFl2vaHint') }}</p>
          </div>
          <span class="h3-download__pill" :data-ready="status?.fl2va_ready ? '1' : '0'">
            {{ status?.fl2va_ready ? t('download.ready') : t('download.notReady') }}
          </span>
        </div>
        <DqButton
          size="sm"
          type="primary"
          :disabled="busy"
          @click="start('fl2va')"
        >
          {{
            busy && activePackage === 'fl2va'
              ? t('download.downloading')
              : status?.fl2va_ready
                ? t('download.resumeOrRepair')
                : t('download.startFl2va')
          }}
        </DqButton>
      </div>

      <div class="h3-download__pkg" data-state="ref2va">
        <div class="h3-download__pkg-head">
          <div>
            <div class="h3-download__pkg-title">{{ t('download.pkgRef2va') }}</div>
            <p class="h3-muted">{{ t('download.pkgRef2vaHint') }}</p>
          </div>
          <span class="h3-download__pill" :data-ready="status?.ref2va_ready ? '1' : '0'">
            {{ status?.ref2va_ready ? t('download.ready') : t('download.notReady') }}
          </span>
        </div>
        <DqButton
          size="sm"
          type="primary"
          :disabled="busy || !status?.fl2va_ready"
          @click="start('ref2va')"
        >
          {{
            busy && activePackage === 'ref2va'
              ? t('download.downloading')
              : status?.ref2va_ready
                ? t('download.resumeOrRepair')
                : t('download.startRef2va')
          }}
        </DqButton>
        <p v-if="!status?.fl2va_ready" class="h3-muted">{{ t('download.needFl2vaFirst') }}</p>
      </div>

      <div class="h3-download__pkg" data-state="turbo_lora">
        <div class="h3-download__pkg-head">
          <div>
            <div class="h3-download__pkg-title">{{ t('download.pkgTurboLora') }}</div>
            <p class="h3-muted">{{ t('download.pkgTurboLoraHint') }}</p>
          </div>
          <span class="h3-download__pill" :data-ready="status?.turbo_lora_ready ? '1' : '0'">
            {{ status?.turbo_lora_ready ? t('download.ready') : t('download.notReady') }}
          </span>
        </div>
        <DqButton
          size="sm"
          type="primary"
          :disabled="busy || !status?.fl2va_ready"
          @click="start('turbo_lora')"
        >
          {{
            busy && activePackage === 'turbo_lora'
              ? t('download.downloading')
              : status?.turbo_lora_ready
                ? t('download.resumeOrRepair')
                : t('download.startTurboLora')
          }}
        </DqButton>
        <p v-if="!status?.fl2va_ready" class="h3-muted">{{ t('download.needFl2vaForTurbo') }}</p>
      </div>
    </section>

    <div v-if="busy && progress" class="h3-download__progress">
      <DqProgress :percentage="progressPct" />
      <p class="h3-muted">{{ progressLine || '…' }}</p>
      <DqButton size="sm" variant="ghost" @click="download.cancel()">
        {{ t('download.cancel') }}
      </DqButton>
    </div>
    <div
      v-else-if="progress && (progress.status === 'failed' || progress.status === 'cancelled')"
      class="h3-download__progress"
    >
      <p class="h3-muted">{{ progress.error || progress.message }}</p>
    </div>

    <div class="h3-download__actions">
      <DqButton size="sm" variant="ghost" :disabled="busy" @click="refresh">
        {{ t('download.refresh') }}
      </DqButton>
    </div>

    <p
      v-if="status?.missing_paths?.length && !status.fl2va_ready"
      class="h3-download__hint"
    >
      {{ t('download.missing') }}: {{ status.missing_paths.slice(0, 4).join(' · ') }}
      <span v-if="status.missing_paths.length > 4">…</span>
    </p>
  </div>
</template>

<style scoped>
.h3-download {
  max-width: 640px;
  display: flex;
  flex-direction: column;
  gap: 24px;
}

.h3-download__head h1 {
  margin: 0;
  font-size: var(--dq-font-size-title, 16px);
  font-weight: 650;
}

.h3-download__head .h3-muted {
  margin: 6px 0 0;
}

.h3-download__source,
.h3-download__packages {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.h3-download__section-label {
  font-size: var(--dq-font-size-caption, 12px);
  font-weight: 600;
  letter-spacing: 0.02em;
  text-transform: uppercase;
  opacity: 0.55;
}

.h3-download__source .h3-muted,
.h3-download__packages .h3-muted {
  margin: 0;
  font-size: var(--dq-font-size-caption, 12px);
}

.h3-download__pkg {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 14px 0;
  border-top: 0.5px solid color-mix(in srgb, var(--dq-label-primary, #fff) 10%, transparent);
}

.h3-download__pkg:first-of-type {
  border-top: none;
  padding-top: 0;
}

.h3-download__pkg-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
}

.h3-download__pkg-title {
  font-weight: 600;
  margin-bottom: 4px;
}

.h3-download__pill {
  flex-shrink: 0;
  font-size: var(--dq-font-size-caption, 12px);
  font-weight: 550;
  opacity: 0.8;
}

.h3-download__pill[data-ready='1'] {
  color: var(--dq-success, #30d158);
}

.h3-download__progress {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.h3-download__progress .h3-muted {
  margin: 0;
  font-size: var(--dq-font-size-caption, 12px);
  word-break: break-all;
}

.h3-download__actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
}

.h3-download__hint {
  margin: 0;
  font-size: var(--dq-font-size-caption, 12px);
  opacity: 0.5;
  line-height: 1.45;
}
</style>

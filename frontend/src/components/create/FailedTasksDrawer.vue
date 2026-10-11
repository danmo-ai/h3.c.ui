<script setup lang="ts">
import { ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { api, type TaskSnapshot } from '@/utils/api';

const props = defineProps<{
  open: boolean;
}>();

const emit = defineEmits<{
  'update:open': [boolean];
}>();

const { t, locale } = useI18n();
const tasks = ref<TaskSnapshot[]>([]);
const loading = ref(false);

async function refresh() {
  loading.value = true;
  try {
    const res = await api.listFailedTasks();
    tasks.value = res.tasks || [];
  } catch {
    tasks.value = [];
  } finally {
    loading.value = false;
  }
}

watch(
  () => props.open,
  (v) => {
    if (v) void refresh();
  },
);

function fmtTime(ms?: number | null) {
  if (!ms) return '—';
  try {
    return new Date(ms).toLocaleString(locale.value === 'zh' ? 'zh-CN' : 'en-US', {
      hour12: false,
    });
  } catch {
    return String(ms);
  }
}

function dims(task: TaskSnapshot) {
  const wh =
    task.width && task.height ? `${task.width}×${task.height}` : '';
  const dur =
    task.seconds != null
      ? `${task.seconds}s`
      : task.frames != null
        ? `${task.frames}f`
        : '';
  return [wh, dur].filter(Boolean).join(' · ');
}
</script>

<template>
  <DqDrawer
    :open="open"
    :title="t('create.failedTasks')"
    direction="rtl"
    size="min(480px, 92vw)"
    @update:open="emit('update:open', $event)"
  >
    <div class="h3-failed">
      <div class="h3-failed__toolbar">
        <DqButton size="sm" variant="ghost" :disabled="loading" @click="refresh">
          {{ t('download.refresh') }}
        </DqButton>
      </div>
      <DqEmpty v-if="!loading && !tasks.length" :description="t('create.failedEmpty')" />
      <div v-else class="h3-failed__list">
        <article v-for="task in tasks" :key="task.task_id" class="h3-failed__item">
          <div class="h3-failed__meta">
            <span>{{ fmtTime(task.finished_ms || task.created_ms) }}</span>
            <span v-if="dims(task)">{{ dims(task) }}</span>
            <span v-if="task.output_name">{{ task.output_name }}</span>
          </div>
          <p class="h3-failed__prompt">{{ task.prompt || '—' }}</p>
          <p class="h3-failed__error">{{ task.error || task.phase || 'failed' }}</p>
          <div class="h3-failed__id">{{ task.task_id }}</div>
        </article>
      </div>
    </div>
  </DqDrawer>
</template>

<style scoped>
.h3-failed {
  display: flex;
  flex-direction: column;
  gap: 14px;
  height: 100%;
  min-height: 0;
}

.h3-failed__toolbar {
  display: flex;
  justify-content: flex-end;
}

.h3-failed__list {
  display: flex;
  flex-direction: column;
  gap: 12px;
  overflow-y: auto;
  min-height: 0;
  padding-bottom: 12px;
}

.h3-failed__item {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 12px 0;
  border-top: 0.5px solid color-mix(in srgb, var(--dq-label-primary, #fff) 10%, transparent);
}

.h3-failed__item:first-child {
  border-top: none;
  padding-top: 0;
}

.h3-failed__meta {
  display: flex;
  flex-wrap: wrap;
  gap: 8px 12px;
  font-size: var(--dq-font-size-caption, 12px);
  opacity: 0.55;
}

.h3-failed__prompt {
  margin: 0;
  font-size: var(--dq-font-size-body, 14px);
  line-height: 1.4;
  opacity: 0.9;
  word-break: break-word;
}

.h3-failed__error {
  margin: 0;
  font-size: var(--dq-font-size-caption, 12px);
  line-height: 1.45;
  color: var(--dq-danger, #ff453a);
  word-break: break-word;
  white-space: pre-wrap;
}

.h3-failed__id {
  font-size: 11px;
  opacity: 0.4;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  word-break: break-all;
}
</style>

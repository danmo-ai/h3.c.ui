import { computed, ref } from 'vue';
import { defineStore } from 'pinia';
import {
  api,
  type DownloadPackageId,
  type DownloadProgress,
  type DownloadSourceId,
} from '@/utils/api';
import { toast } from '@/utils/feedback';
import i18n from '@/utils/i18n';

export const useDownloadStore = defineStore('download', () => {
  const progress = ref<DownloadProgress | null>(null);
  const busy = ref(false);
  const source = ref<DownloadSourceId>('modelscope');
  const activePackage = ref<DownloadPackageId | null>(null);
  let es: EventSource | null = null;
  let pollTimer: ReturnType<typeof setInterval> | null = null;

  const active = computed(
    () =>
      busy.value &&
      !!progress.value &&
      (progress.value.status === 'queued' || progress.value.status === 'running'),
  );

  function stopStream() {
    es?.close();
    es = null;
    if (pollTimer) {
      clearInterval(pollTimer);
      pollTimer = null;
    }
  }

  function startPoll(taskId: string) {
    if (pollTimer) clearInterval(pollTimer);
    pollTimer = setInterval(() => {
      void api
        .downloadCurrent()
        .then((cur) => {
          if (!cur.task_id || cur.task_id !== taskId) return;
          progress.value = cur;
          if (
            cur.status === 'completed' ||
            cur.status === 'failed' ||
            cur.status === 'cancelled'
          ) {
            onTerminal(cur);
          }
        })
        .catch(() => {
          /* keep last snapshot */
        });
    }, 1000);
  }

  function onTerminal(data: DownloadProgress) {
    stopStream();
    busy.value = false;
    activePackage.value = null;
    progress.value = data;
    if (data.status === 'completed') {
      toast.success(data.message || 'Download complete');
    } else if (data.status === 'failed') {
      toast.error(data.error || data.message || data.status);
    } else if (data.status === 'cancelled') {
      toast.notify({
        title: String(i18n.global.t('download.cancelled')),
        message: '',
        type: 'warning',
      });
    }
  }

  function attachStream(taskId: string) {
    stopStream();
    busy.value = true;
    es = new EventSource(api.downloadStreamUrl(taskId));
    es.addEventListener('progress', (ev) => {
      const data = JSON.parse((ev as MessageEvent).data) as DownloadProgress;
      progress.value = data;
      if (
        data.status === 'completed' ||
        data.status === 'failed' ||
        data.status === 'cancelled'
      ) {
        onTerminal(data);
      }
    });
    es.onerror = () => {
      // SSE can drop when the tab/background throttles — fall back to polling.
      if (busy.value) startPoll(taskId);
    };
    startPoll(taskId);
  }

  async function restore() {
    try {
      const cur = await api.downloadCurrent();
      if (cur.task_id && (cur.status === 'queued' || cur.status === 'running')) {
        progress.value = cur;
        if (!es) attachStream(cur.task_id);
        else busy.value = true;
        return true;
      }
    } catch {
      /* endpoint missing on old API */
    }
    return false;
  }

  async function start(packageId: DownloadPackageId = 'fl2va') {
    if (busy.value) return;
    busy.value = true;
    activePackage.value = packageId;
    progress.value = null;
    try {
      const { task_id } = await api.startDownload(source.value, packageId);
      attachStream(task_id);
    } catch (e) {
      busy.value = false;
      activePackage.value = null;
      toast.error(String((e as Error).message || e));
      throw e;
    }
  }

  async function cancel() {
    const id = progress.value?.task_id;
    if (!id) return;
    await api.cancelDownload(id);
  }

  function setSource(id: DownloadSourceId) {
    if (!busy.value) source.value = id;
  }

  return {
    progress,
    busy,
    source,
    activePackage,
    active,
    attachStream,
    restore,
    start,
    cancel,
    setSource,
    stopStream,
  };
});

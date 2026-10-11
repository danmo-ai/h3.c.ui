<script setup lang="ts">
import { useI18n } from 'vue-i18n';
import { VideoCamera } from '@danqing/dq-shell';

defineProps<{ busy?: boolean }>();
const emit = defineEmits<{ open: [] }>();
const { t } = useI18n();
</script>

<template>
  <button
    type="button"
    class="h3-compose-fab"
    :disabled="busy"
    :aria-label="t('create.openComposer')"
    @click="emit('open')"
  >
    <DqIcon :size="22"><VideoCamera /></DqIcon>
    <span>{{ t('create.openComposer') }}</span>
  </button>
</template>

<style scoped>
.h3-compose-fab {
  position: fixed;
  left: calc(var(--dq-shell-sidebar-width, 60px) + 50%);
  bottom: calc(20px + env(safe-area-inset-bottom, 0px));
  transform: translateX(-50%);
  z-index: 40;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 12px 18px;
  border: 0.5px solid var(--dq-glass-border, transparent);
  border-radius: 999px;
  background: var(--dq-glass-floating-bar-bg, var(--dq-fill-secondary, #2a2a2c));
  color: var(--dq-label-primary);
  font: inherit;
  font-size: var(--dq-font-size-body, 14px);
  font-weight: 600;
  cursor: pointer;
  box-shadow: var(--dq-shadow-md, 0 8px 28px rgba(0, 0, 0, 0.28));
  backdrop-filter: var(--dq-glass-blur-heavy, blur(20px));
  -webkit-backdrop-filter: var(--dq-glass-blur-heavy, blur(20px));
}

.h3-compose-fab:hover:not(:disabled) {
  background: color-mix(in srgb, var(--dq-accent) 18%, var(--dq-fill-secondary, #2a2a2c));
}

.h3-compose-fab:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}
</style>

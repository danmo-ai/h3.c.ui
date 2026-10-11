<script setup lang="ts">
import { computed, type Component } from 'vue';
import { storeToRefs } from 'pinia';
import { useI18n } from 'vue-i18n';
import { useRoute, useRouter } from 'vue-router';
import { Box, MagicStick, Setting, VideoCamera } from '@danqing/dq-shell';
import { useDownloadStore } from '@/stores/download';

const { t } = useI18n();
const route = useRoute();
const router = useRouter();
const download = useDownloadStore();
const { active: downloadActive } = storeToRefs(download);

interface NavItem {
  id: string;
  icon: Component;
  label: string;
  routeName: string;
}

const items = computed<NavItem[]>(() => [
  { id: 'create', icon: VideoCamera, label: t('app.create'), routeName: 'create' },
  { id: 'download', icon: Box, label: t('app.download'), routeName: 'download' },
  { id: 'settings', icon: Setting, label: t('app.settings'), routeName: 'settings' },
]);

function go(name: string) {
  void router.push({ name });
}
</script>

<template>
  <div class="dq-sidebar-nav">
    <div class="dq-sidebar-nav__brand" :title="t('app.name')">
      <DqIcon :size="22"><MagicStick /></DqIcon>
    </div>

    <nav class="dq-sidebar-nav__group" role="navigation" :aria-label="t('app.name')">
      <button
        v-for="item in items"
        :key="item.id"
        type="button"
        class="dq-sidebar-nav__item"
        :class="{
          'is-active': route.name === item.routeName,
          'is-busy': item.id === 'download' && downloadActive,
        }"
        @click="go(item.routeName)"
      >
        <DqIcon :size="22"><component :is="item.icon" /></DqIcon>
        <span v-if="item.id === 'download' && downloadActive" class="dq-sidebar-nav__dot" />
        <span class="dq-sidebar-nav__tooltip">{{ item.label }}</span>
      </button>
    </nav>

    <div class="dq-sidebar-nav__spacer" aria-hidden="true" />
  </div>
</template>

<style scoped>
.dq-sidebar-nav {
  display: flex;
  flex-direction: column;
  align-items: center;
  height: 100%;
  width: 100%;
  padding: 14px 0 16px;
  box-sizing: border-box;
}

.dq-sidebar-nav__brand {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  margin-bottom: 18px;
  color: var(--dq-accent);
  flex-shrink: 0;
}

.dq-sidebar-nav__group {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
  width: 100%;
  flex-shrink: 0;
}

.dq-sidebar-nav__spacer {
  flex: 1;
  width: 100%;
  min-height: 8px;
}

.dq-sidebar-nav__item {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 36px;
  padding: 0;
  margin: 0;
  border: none;
  border-radius: 10px;
  background: transparent;
  color: var(--dq-label-secondary);
  cursor: pointer;
  transition:
    color 0.2s cubic-bezier(0.4, 0, 0.2, 1),
    background 0.2s cubic-bezier(0.4, 0, 0.2, 1),
    transform 0.15s ease;
  font-family: inherit;
  flex-shrink: 0;
}

.dq-sidebar-nav__item:hover {
  color: var(--dq-label-primary);
  background: var(--dq-fill-tertiary);
}

.dq-sidebar-nav__item.is-active {
  color: var(--dq-accent);
  background: color-mix(in srgb, var(--dq-accent) 16%, transparent);
}

.dq-sidebar-nav__item:active {
  transform: scale(0.94);
}

.dq-sidebar-nav__item.is-busy {
  color: var(--dq-accent);
}

.dq-sidebar-nav__dot {
  position: absolute;
  top: 6px;
  right: 6px;
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--dq-accent);
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--dq-material-chrome, #1c1c1e) 80%, transparent);
}

.dq-sidebar-nav__tooltip {
  position: absolute;
  left: calc(100% + 12px);
  top: 50%;
  transform: translateY(-50%) scale(0.95);
  padding: 6px 10px;
  background: var(--dq-glass-tooltip-bg, var(--dq-fill-secondary, #2a2a2c));
  border: 0.5px solid var(--dq-glass-border, transparent);
  border-radius: 7px;
  font-size: var(--dq-font-size-caption, 12px);
  font-weight: 500;
  color: var(--dq-label-primary);
  white-space: nowrap;
  pointer-events: none;
  opacity: 0;
  visibility: hidden;
  transition:
    opacity 0.12s ease,
    transform 0.12s ease,
    visibility 0.12s;
  box-shadow: var(--dq-shadow-md, 0 4px 16px rgba(0, 0, 0, 0.25));
  backdrop-filter: var(--dq-glass-blur-light, blur(12px));
  -webkit-backdrop-filter: var(--dq-glass-blur-light, blur(12px));
  z-index: 1000;
}

.dq-sidebar-nav__item:hover .dq-sidebar-nav__tooltip {
  opacity: 1;
  visibility: visible;
  transform: translateY(-50%) scale(1);
}
</style>

<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { api, type ModelStatus } from '@/utils/api';
import { getItem, setItem } from '@/utils/storage';
import ComposeFab from '@/components/create/ComposeFab.vue';
import ComposerDrawer from '@/components/create/ComposerDrawer.vue';
import CreateCard from '@/components/create/CreateCard.vue';
import CreateComposer from '@/components/create/CreateComposer.vue';

const DRAWER_KEY = 'composer.drawer.open';

const { t } = useI18n();
const status = ref<ModelStatus | null>(null);
const outputs = ref<Array<{ name: string; bytes: number }>>([]);
const drawerOpen = ref(getItem(DRAWER_KEY) === '1');
const composerBusy = ref(false);
const composerRef = ref<{ closeEs?: () => void } | null>(null);

async function refreshStatus() {
  status.value = await api.modelStatus();
}

async function refreshOutputs() {
  const res = await api.listOutputs();
  outputs.value = res.outputs.map((o) => ({ name: o.name, bytes: o.bytes }));
}

function openDrawer() {
  drawerOpen.value = true;
  setItem(DRAWER_KEY, '1');
}

function onDrawerOpen(v: boolean) {
  drawerOpen.value = v;
  setItem(DRAWER_KEY, v ? '1' : '0');
}

async function removeOutput(name: string) {
  await api.deleteOutput(name);
  await refreshOutputs();
}

onMounted(async () => {
  await refreshStatus();
  await refreshOutputs();
});

onUnmounted(() => {
  composerRef.value?.closeEs?.();
});
</script>

<template>
  <div class="h3-create-page">
    <header class="h3-create-page__head">
      <h1>{{ t('create.title') }}</h1>
      <p class="h3-muted">{{ t('create.galleryHint') }}</p>
    </header>

    <DqEmpty v-if="!outputs.length" :description="t('create.galleryEmpty')" />
    <div v-else class="h3-studio-grid">
      <CreateCard
        v-for="o in outputs"
        :key="o.name"
        :name="o.name"
        :bytes="o.bytes"
        @remove="removeOutput(o.name)"
      />
    </div>

    <ComposeFab v-if="!drawerOpen" :busy="composerBusy" @open="openDrawer" />

    <ComposerDrawer
      :open="drawerOpen"
      :title="t('create.composerTitle')"
      @update:open="onDrawerOpen"
    >
      <CreateComposer
        ref="composerRef"
        :status="status"
        @generated="refreshOutputs"
        @update:busy="(v) => (composerBusy = v)"
      />
    </ComposerDrawer>
  </div>
</template>

<style scoped>
.h3-create-page {
  min-height: 100%;
  padding-bottom: 96px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.h3-create-page__head h1 {
  margin: 0;
  font-size: var(--dq-font-size-title, 16px);
  font-weight: 650;
}

.h3-studio-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(168px, 1fr));
  gap: 14px 16px;
}

@media (min-width: 768px) {
  .h3-studio-grid {
    grid-template-columns: repeat(auto-fill, minmax(188px, 1fr));
  }
}
</style>

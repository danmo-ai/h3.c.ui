<script setup lang="ts">
defineProps<{
  open: boolean;
  title: string;
}>();

const emit = defineEmits<{
  'update:open': [boolean];
}>();

const drawerSize = 'min(560px, 92vw)';
</script>

<template>
  <DqDrawer
    :open="open"
    :title="title"
    direction="rtl"
    :size="drawerSize"
    class="h3-composer-drawer"
    @update:open="emit('update:open', $event)"
  >
    <div class="h3-composer-drawer__body">
      <slot />
    </div>
  </DqDrawer>
</template>

<style scoped>
.h3-composer-drawer__body {
  display: flex;
  flex-direction: column;
  min-height: 0;
  height: 100%;
  overflow-y: auto;
  padding: 4px 2px 20px;
  box-sizing: border-box;
}

:deep(.dq-drawer-body) {
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
  background: transparent;
}
</style>

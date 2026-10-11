<script setup lang="ts">
import SideNav from '@/components/shell/SideNav.vue';
</script>

<template>
  <div class="main-container h3-shell dq-app-shell dq-app-shell--sidebar">
    <aside class="h3-sidebar app-sidebar dq-app-sidebar dq-glass--sidebar">
      <div class="dq-tauri-titlebar-inset" aria-hidden="true" />
      <SideNav />
    </aside>
    <main class="h3-main app-main dq-app-main">
      <router-view />
    </main>
  </div>
</template>

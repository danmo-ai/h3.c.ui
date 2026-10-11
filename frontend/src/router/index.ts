import { createRouter, createWebHashHistory } from 'vue-router';
import CreateView from '@/views/CreateView.vue';
import DownloadView from '@/views/DownloadView.vue';
import SettingsView from '@/views/SettingsView.vue';

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: '/', redirect: '/create' },
    { path: '/create', name: 'create', component: CreateView },
    { path: '/download', name: 'download', component: DownloadView },
    { path: '/settings', name: 'settings', component: SettingsView },
  ],
});

export default router;

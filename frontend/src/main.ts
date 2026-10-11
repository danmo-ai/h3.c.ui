import { createApp } from 'vue';
import { createPinia } from 'pinia';
import { registerDqIcons } from '@danqing/dq-shell';
import router from './router';
import i18n from './utils/i18n';
import { toast } from './utils/feedback';
import App from './App.vue';
import { installDanQingUi } from './plugins/dq-ui';
import { installTauriMacosShell } from './utils/desktop';
import { useThemeStore } from './stores/theme';
import { useDownloadStore } from './stores/download';
import '@danqing/dq-tokens/dq-mac.css';
import '@danqing/dq-tokens/dq-mac-light.css';
import '@danqing/dq-tokens/dq-tokyo-night.css';
import '@danqing/dq-tokens/dq-nord-dark.css';
import '@danqing/dq-tokens/dq-minimal-light.css';
import '@danqing/dq-tokens/dq-recipe-dark.css';
import '@danqing/dq-tokens/dq-recipe-light.css';
import '@danqing/dq-tokens/dq-glass.css';
import '@danqing/dq-tokens/dq-agent.css';
import '@danqing/dq-ui/style.css';
import '@danqing/dq-shell/style.css';
import './styles/theme.css';
import '@danqing/dq-tokens/dq-tauri-macos.css';

document.documentElement.classList.add('dq-mac', 'dark');
installTauriMacosShell();

const app = createApp(App);
// Linked dq-* packages may resolve a second Vue copy; cast keeps register typed.
registerDqIcons(app as Parameters<typeof registerDqIcons>[0]);
app.use(createPinia());
app.use(router);
installDanQingUi(app as Parameters<typeof installDanQingUi>[0]);
app.use(i18n);

const themeStore = useThemeStore();
themeStore.init();

app.mount('#app');

// Reattach in-flight model download across route changes / reloads.
void useDownloadStore().restore();

app.config.errorHandler = (err) => {
  console.error(err);
  toast.notify({
    title: 'Error',
    message: String((err as Error)?.message || err).slice(0, 200),
    type: 'error',
    duration: 6000,
  });
};

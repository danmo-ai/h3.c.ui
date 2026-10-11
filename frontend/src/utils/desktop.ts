export function isTauriRuntime(): boolean {
  const w = window as Window & { __TAURI_INTERNALS__?: unknown; __TAURI__?: unknown };
  return Boolean(w.__TAURI_INTERNALS__ ?? w.__TAURI__);
}

export function installTauriMacosShell(): void {
  if (!isTauriRuntime()) return;
  const platform = navigator.platform.toLowerCase();
  const ua = navigator.userAgent.toLowerCase();
  if (!platform.includes('mac') && !ua.includes('mac')) return;
  document.documentElement.classList.add('dq-tauri-macos');
}

export function syncTauriWindowTheme(dark: boolean): void {
  const scheme = dark ? 'dark' : 'light';
  document.documentElement.style.colorScheme = scheme;
  if (!isTauriRuntime()) return;
  void import('@tauri-apps/api/window')
    .then(({ getCurrentWindow }) => getCurrentWindow().setTheme(scheme))
    .catch((err: unknown) => {
      console.error('Failed to sync Tauri window theme:', err);
    });
}

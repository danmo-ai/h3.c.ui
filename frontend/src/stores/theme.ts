import { defineStore } from 'pinia';
import { ref, watch } from 'vue';
import {
  THEME_OPTIONS as DQ_THEME_OPTIONS,
  applyDqTheme,
  getDqThemeMeta,
  resolveDqThemeSlug,
  type DqThemeSlug,
} from '@danqing/dq-tokens';
import { getItem, setItem, H3_STORAGE } from '@/utils/storage';
import { syncTauriWindowTheme } from '@/utils/desktop';

export type ThemeId = DqThemeSlug;

export const THEME_OPTIONS = DQ_THEME_OPTIONS.map((opt) => ({
  id: opt.slug as ThemeId,
  label: opt.label,
  description: opt.description,
  dark: opt.dark,
}));

export function applyTheme(themeId?: ThemeId | string | null): void {
  const id = (resolveDqThemeSlug(themeId ?? undefined) ?? 'mac') as ThemeId;
  applyDqTheme(id);
  const dark = getDqThemeMeta(id)?.dark ?? true;
  syncTauriWindowTheme(dark);
}

export const useThemeStore = defineStore('theme', () => {
  const currentTheme = ref<ThemeId>(
    (resolveDqThemeSlug(getItem(H3_STORAGE.THEME) ?? undefined) ?? 'mac') as ThemeId,
  );

  function setTheme(id: ThemeId) {
    const next = (resolveDqThemeSlug(id) ?? 'mac') as ThemeId;
    currentTheme.value = next;
    applyTheme(next);
    setItem(H3_STORAGE.THEME, next);
  }

  function init() {
    applyTheme(currentTheme.value);
    setItem(H3_STORAGE.THEME, currentTheme.value);
  }

  watch(currentTheme, (id) => {
    applyTheme(id);
    setItem(H3_STORAGE.THEME, id);
  });

  return { currentTheme, setTheme, init };
});

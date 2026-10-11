const PREFIX = 'h3-ui.';

export function getItem(key: string): string | null {
  try {
    return localStorage.getItem(PREFIX + key);
  } catch {
    return null;
  }
}

export function setItem(key: string, value: string): void {
  try {
    localStorage.setItem(PREFIX + key, value);
  } catch {
    /* ignore */
  }
}

export const H3_STORAGE = {
  THEME: 'theme',
  LOCALE: 'locale',
} as const;

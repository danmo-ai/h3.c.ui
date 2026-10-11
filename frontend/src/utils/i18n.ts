import { createI18n } from 'vue-i18n';
import en from '@/locales/en.json';
import zh from '@/locales/zh.json';
import { getItem, setItem, H3_STORAGE } from '@/utils/storage';

export type LocaleId = 'en' | 'zh';

const saved = (getItem(H3_STORAGE.LOCALE) as LocaleId | null) || 'zh';

const i18n = createI18n({
  legacy: false,
  locale: saved,
  fallbackLocale: 'en',
  messages: { en, zh },
});

export function setLocale(locale: LocaleId) {
  i18n.global.locale.value = locale;
  setItem(H3_STORAGE.LOCALE, locale);
}

export default i18n;

import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import LanguageDetector from 'i18next-browser-languagedetector';
import zhCN from './zh-CN.json';
import en from './en.json';

// Register resources under both the short ISO code ("zhcn", "en") used by the
// backend Settings enum and the conventional BCP-47 names ("zh-CN", "en").
i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    resources: {
      'zh-CN': { translation: zhCN },
      zhcn: { translation: zhCN },
      en: { translation: en },
    },
    fallbackLng: 'zhcn',
    supportedLngs: ['zh-CN', 'zhcn', 'en'],
    nonExplicitSupportedLngs: true,
    interpolation: { escapeValue: false },
    detection: {
      order: ['localStorage', 'navigator'],
      caches: ['localStorage'],
      lookupLocalStorage: 'jadb-language',
    },
  });

/** Map the backend enum ('zhcn' | 'en') to an i18next locale. */
export function localeFor(lang: 'zhcn' | 'en'): 'zh-CN' | 'en' {
  return lang === 'zhcn' ? 'zh-CN' : 'en';
}

export default i18n;

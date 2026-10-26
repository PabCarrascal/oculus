import i18next from "i18next";
import { initReactI18next } from "react-i18next";
import en from "./en.json";
import es from "./es.json";

export const resources = { es: { translation: es }, en: { translation: en } } as const;
export type AppLocale = keyof typeof resources;

declare module "i18next" {
  interface CustomTypeOptions {
    // Las claves se comprueban al compilar contra el catálogo en español.
    resources: { translation: typeof es };
  }
}

export function initI18n(locale: AppLocale) {
  return i18next.use(initReactI18next).init({
    resources,
    lng: locale,
    fallbackLng: "en",
    interpolation: { escapeValue: false },
    returnNull: false,
  });
}

export function setLocale(locale: AppLocale) {
  if (i18next.language !== locale) {
    void i18next.changeLanguage(locale);
  }
  document.documentElement.lang = locale;
}

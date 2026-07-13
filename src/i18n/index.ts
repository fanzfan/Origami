import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import { enUS } from "./locales/en-US";
import { zhCN } from "./locales/zh-CN";

export type AppLanguage = "system" | "zh-CN" | "en-US";
export type ResolvedLanguage = Exclude<AppLanguage, "system">;

export const LANGUAGE_OPTIONS: ReadonlyArray<{ value: AppLanguage; nativeName: string | null }> = [
  { value: "system", nativeName: null },
  { value: "zh-CN", nativeName: "简体中文" },
  { value: "en-US", nativeName: "English" },
];

export function isAppLanguage(value: unknown): value is AppLanguage {
  return LANGUAGE_OPTIONS.some((option) => option.value === value);
}

export function resolveLanguage(preference: AppLanguage): ResolvedLanguage {
  if (preference !== "system") return preference;
  const languages = navigator.languages?.length ? navigator.languages : [navigator.language];
  return languages.some((language) => language.toLowerCase().startsWith("zh")) ? "zh-CN" : "en-US";
}

i18n.use(initReactI18next).init({
  resources: {
    "zh-CN": { translation: zhCN },
    "en-US": { translation: enUS },
  },
  lng: resolveLanguage("system"),
  fallbackLng: "en-US",
  supportedLngs: ["zh-CN", "en-US"],
  load: "currentOnly",
  initAsync: false,
  returnNull: false,
  interpolation: {
    escapeValue: false,
  },
});

export function applyLanguage(preference: AppLanguage) {
  const language = resolveLanguage(preference);
  document.documentElement.lang = language;
  return i18n.changeLanguage(language);
}

export default i18n;

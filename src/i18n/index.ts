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

/**
 * 把后端返回的哨兵错误翻成人话，其余原样返回。
 * 后端只发稳定的标识（可带冒号后的参数），文案全部留在前端资源里。
 */
export function explainError(message: string): string {
  if (message.startsWith("MISSING_VOLUMES:")) {
    return i18n.t("app.error.reason.missingVolumes", {
      names: message.slice("MISSING_VOLUMES:".length).trim(),
    });
  }
  if (message === "SPANNED_ZIP_UNSUPPORTED") return i18n.t("app.error.reason.spannedZip");
  if (message === "RAR_VOLUME_REQUIRED") return i18n.t("app.error.reason.rarVolumeRequired");
  if (message.startsWith("RAR_PREVIEW_TOO_LARGE:")) return i18n.t("app.error.reason.rarPreviewTooLarge");
  if (message === "SPLIT_ARCHIVE_READONLY") return i18n.t("app.error.reason.splitReadonly");
  return message;
}

export function applyLanguage(preference: AppLanguage) {
  const language = resolveLanguage(preference);
  document.documentElement.lang = language;
  return i18n.changeLanguage(language);
}

export default i18n;

import { invoke, isTauri } from "@tauri-apps/api/core"
import i18n from "i18next"
import { initReactI18next } from "react-i18next"

import enCommon from "@/locales/en/common.json"
import enNavigation from "@/locales/en/navigation.json"
import enPages from "@/locales/en/pages.json"
import ruCommon from "@/locales/ru/common.json"
import ruNavigation from "@/locales/ru/navigation.json"
import ruPages from "@/locales/ru/pages.json"
import zhCNCommon from "@/locales/zh-CN/common.json"
import zhCNNavigation from "@/locales/zh-CN/navigation.json"
import zhCNPages from "@/locales/zh-CN/pages.json"
import zhTWCommon from "@/locales/zh-TW/common.json"
import zhTWNavigation from "@/locales/zh-TW/navigation.json"
import zhTWPages from "@/locales/zh-TW/pages.json"

export const SUPPORTED_LANGS = [
  "en",
  "ru",
  "zh-CN",
  "zh-TW",
] as const

export type LangCode = (typeof SUPPORTED_LANGS)[number]

const STORAGE_KEY = "gproxy-console-lang"
const langCodes = new Set<LangCode>(SUPPORTED_LANGS)
const combine = (...domains: Array<object>) => Object.assign({}, ...domains)

function storedLanguage(): LangCode {
  try {
    const value = window.localStorage.getItem(STORAGE_KEY)
    if (value && langCodes.has(value as LangCode)) return value as LangCode
  } catch {
    // A blocked preference store must not prevent browser-language detection.
  }
  for (const language of navigator.languages.length ? navigator.languages : [navigator.language]) {
    const code = language.toLowerCase()
    if (code === "ru" || code.startsWith("ru-")) return "ru"
    if (code === "en" || code.startsWith("en-")) return "en"
    if (code === "zh" || code.startsWith("zh-")) {
      return /^zh-(hant|tw|hk|mo)(-|$)/.test(code) ? "zh-TW" : "zh-CN"
    }
  }
  return "en"
}

void i18n.use(initReactI18next).init({
  lng: typeof window === "undefined" ? "en" : storedLanguage(),
  fallbackLng: "en",
  resources: {
    en: { translation: combine(enCommon, enNavigation, enPages) },
    ru: { translation: combine(ruCommon, ruNavigation, ruPages) },
    "zh-CN": { translation: combine(zhCNCommon, zhCNNavigation, zhCNPages) },
    "zh-TW": { translation: combine(zhTWCommon, zhTWNavigation, zhTWPages) },
  },
  interpolation: { escapeValue: false },
})

function syncShellLanguage(language: string) {
  if (isTauri()) void invoke("desktop_preferences_language", { language }).catch(error => console.warn("Could not update the application language", error))
}

syncShellLanguage(i18n.language)
i18n.on("languageChanged", (language) => {
  syncShellLanguage(language)
  if (typeof document !== "undefined") document.documentElement.lang = language
})

if (typeof document !== "undefined") document.documentElement.lang = i18n.language

export function setLanguage(code: LangCode) {
  try {
    window.localStorage.setItem(STORAGE_KEY, code)
  } catch {
    // Storage may be unavailable in hardened browsers; the in-memory locale still changes.
  }
  return i18n.changeLanguage(code)
}

export default i18n

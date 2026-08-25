import i18n from "i18next"
import { initReactI18next } from "react-i18next"

import en from "@/locales/en.json"
import zhCN from "@/locales/zh-CN.json"
import zhTW from "@/locales/zh-TW.json"

export const SUPPORTED_LANGS = [
  "en",
  "zh-CN",
  "zh-TW",
] as const

export type LangCode = (typeof SUPPORTED_LANGS)[number]

const STORAGE_KEY = "gproxy-console-lang"
const langCodes = new Set<LangCode>(SUPPORTED_LANGS)

function storedLanguage(): LangCode {
  try {
    const value = window.localStorage.getItem(STORAGE_KEY)
    return value && langCodes.has(value as LangCode) ? (value as LangCode) : "en"
  } catch {
    return "en"
  }
}

void i18n.use(initReactI18next).init({
  lng: typeof window === "undefined" ? "en" : storedLanguage(),
  fallbackLng: "en",
  resources: {
    en: { translation: en },
    "zh-CN": { translation: zhCN },
    "zh-TW": { translation: zhTW },
  },
  interpolation: { escapeValue: false },
})

i18n.on("languageChanged", (language) => {
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

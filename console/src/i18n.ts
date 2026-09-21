import i18n from "i18next"
import { initReactI18next } from "react-i18next"

import enCommon from "@/locales/en/common.json"
import enNavigation from "@/locales/en/navigation.json"
import enPages from "@/locales/en/pages.json"
import zhCNCommon from "@/locales/zh-CN/common.json"
import zhCNNavigation from "@/locales/zh-CN/navigation.json"
import zhCNPages from "@/locales/zh-CN/pages.json"
import zhTWCommon from "@/locales/zh-TW/common.json"
import zhTWNavigation from "@/locales/zh-TW/navigation.json"
import zhTWPages from "@/locales/zh-TW/pages.json"

export const SUPPORTED_LANGS = [
  "en",
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
    return value && langCodes.has(value as LangCode) ? (value as LangCode) : "en"
  } catch {
    return "en"
  }
}

void i18n.use(initReactI18next).init({
  lng: typeof window === "undefined" ? "en" : storedLanguage(),
  fallbackLng: "en",
  resources: {
    en: { translation: combine(enCommon, enNavigation, enPages) },
    "zh-CN": { translation: combine(zhCNCommon, zhCNNavigation, zhCNPages) },
    "zh-TW": { translation: combine(zhTWCommon, zhTWNavigation, zhTWPages) },
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

import i18n from "i18next"
import { initReactI18next } from "react-i18next"

import enCommon from "@/locales/en/common.json"
import enIdentity from "@/locales/en/identity.json"
import enObservability from "@/locales/en/observability.json"
import enPortal from "@/locales/en/portal.json"
import enPricing from "@/locales/en/pricing.json"
import enProviders from "@/locales/en/providers.json"
import enRoutes from "@/locales/en/routes.json"
import enRules from "@/locales/en/rules.json"
import enSettings from "@/locales/en/settings.json"
import enUpdate from "@/locales/en/update.json"
import zhCNCommon from "@/locales/zh-CN/common.json"
import zhCNIdentity from "@/locales/zh-CN/identity.json"
import zhCNObservability from "@/locales/zh-CN/observability.json"
import zhCNPortal from "@/locales/zh-CN/portal.json"
import zhCNPricing from "@/locales/zh-CN/pricing.json"
import zhCNProviders from "@/locales/zh-CN/providers.json"
import zhCNRoutes from "@/locales/zh-CN/routes.json"
import zhCNRules from "@/locales/zh-CN/rules.json"
import zhCNSettings from "@/locales/zh-CN/settings.json"
import zhCNUpdate from "@/locales/zh-CN/update.json"
import zhTWCommon from "@/locales/zh-TW/common.json"
import zhTWIdentity from "@/locales/zh-TW/identity.json"
import zhTWObservability from "@/locales/zh-TW/observability.json"
import zhTWPortal from "@/locales/zh-TW/portal.json"
import zhTWPricing from "@/locales/zh-TW/pricing.json"
import zhTWProviders from "@/locales/zh-TW/providers.json"
import zhTWRoutes from "@/locales/zh-TW/routes.json"
import zhTWRules from "@/locales/zh-TW/rules.json"
import zhTWSettings from "@/locales/zh-TW/settings.json"
import zhTWUpdate from "@/locales/zh-TW/update.json"

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
    en: { translation: combine(enCommon, enIdentity, enObservability, enPortal, enPricing, enProviders, enRoutes, enRules, enSettings, enUpdate) },
    "zh-CN": { translation: combine(zhCNCommon, zhCNIdentity, zhCNObservability, zhCNPortal, zhCNPricing, zhCNProviders, zhCNRoutes, zhCNRules, zhCNSettings, zhCNUpdate) },
    "zh-TW": { translation: combine(zhTWCommon, zhTWIdentity, zhTWObservability, zhTWPortal, zhTWPricing, zhTWProviders, zhTWRoutes, zhTWRules, zhTWSettings, zhTWUpdate) },
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

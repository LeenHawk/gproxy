import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import resourcesToBackend from "i18next-resources-to-backend";

export const SUPPORTED_LANGS = [
  { code: "en", label: "English" },
  { code: "zh-CN", label: "简体中文" },
  { code: "zh-TW", label: "繁體中文" },
] as const;
export type LangCode = (typeof SUPPORTED_LANGS)[number]["code"];

const STORAGE_KEY = "gproxy-console-lang";

void i18n
  .use(
    resourcesToBackend(
      (lng: string, ns: string) => import(`../locales/${lng}/${ns}.json`),
    ),
  )
  .use(initReactI18next)
  .init({
    lng: (() => { try { return localStorage.getItem(STORAGE_KEY); } catch { return null; } })() ?? "en",
    fallbackLng: "en",
    ns: ["common"],
    defaultNS: "common",
    interpolation: { escapeValue: false },
  });

i18n.on("languageChanged", (lng) => {
  document.documentElement.lang = lng;
});

export function setLanguage(code: LangCode) {
  try { localStorage.setItem(STORAGE_KEY, code); } catch { /* ignore */ }
  void i18n.changeLanguage(code);
}

export default i18n;

import { createContext, useContext, useMemo, useState, type ReactNode } from "react";

import { enMessages } from "./messages/en";
import { zhMessages } from "./messages/zh";

export type Locale = "en" | "zh";

type I18nValue = {
  locale: Locale;
  setLocale: (locale: Locale) => void;
  t: (key: string, params?: Record<string, string | number>) => string;
};

const LOCALE_STORAGE_KEY = "gproxy_console_locale";

const MESSAGES: Record<Locale, Record<string, string>> = {
  en: enMessages,
  zh: zhMessages,
};

const I18nContext = createContext<I18nValue | null>(null);

function readStoredLocale(): Locale {
  if (typeof window === "undefined") {
    return "en";
  }
  const raw = window.localStorage.getItem(LOCALE_STORAGE_KEY);
  if (raw === "zh" || raw === "en") {
    return raw;
  }
  const browserLocales =
    navigator.languages.length > 0 ? navigator.languages : [navigator.language];
  return browserLocales.some((item) => item.toLowerCase().startsWith("zh")) ? "zh" : "en";
}

function formatTemplate(template: string, params?: Record<string, string | number>) {
  if (!params) {
    return template;
  }
  return template.replace(/\{(\w+)\}/g, (_, key: string) => {
    const value = params[key];
    return value === undefined ? `{${key}}` : String(value);
  });
}

export function I18nProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState<Locale>(() => readStoredLocale());

  const setLocale = (next: Locale) => {
    if (typeof window !== "undefined") {
      window.localStorage.setItem(LOCALE_STORAGE_KEY, next);
    }
    setLocaleState(next);
  };

  const value = useMemo<I18nValue>(() => {
    const t = (key: string, params?: Record<string, string | number>) => {
      const table = MESSAGES[locale] ?? MESSAGES.en;
      const fallback = MESSAGES.en[key] ?? key;
      return formatTemplate(table[key] ?? fallback, params);
    };
    return { locale, setLocale, t };
  }, [locale]);

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18nValue {
  const value = useContext(I18nContext);
  if (!value) {
    throw new Error("useI18n must be used under I18nProvider");
  }
  return value;
}

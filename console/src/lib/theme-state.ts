export type Theme = "light" | "dark" | "system"
export type ResolvedTheme = Exclude<Theme, "system">

export const THEME_STORAGE_KEY = "gproxy-console-theme"
const themes = new Set<Theme>(["light", "dark", "system"])

export function resolveTheme(theme: Theme, systemDark: boolean): ResolvedTheme {
  return theme === "system" ? (systemDark ? "dark" : "light") : theme
}

export function storedTheme(storage: Pick<Storage, "getItem"> | null): Theme {
  try {
    const value = storage?.getItem(THEME_STORAGE_KEY)
    return value && themes.has(value as Theme) ? (value as Theme) : "system"
  } catch {
    return "system"
  }
}

export function systemPrefersDark() {
  return window.matchMedia("(prefers-color-scheme: dark)").matches
}

export function applyTheme(theme: Theme) {
  document.documentElement.classList.toggle("dark", resolveTheme(theme, systemPrefersDark()) === "dark")
}

export function applyInitialTheme() {
  if (typeof window !== "undefined") applyTheme(storedTheme(window.localStorage))
}

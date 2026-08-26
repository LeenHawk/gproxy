import { useEffect, useMemo, useState, type ReactNode } from "react"
import { ThemeContext, type ThemeContextValue } from "@/lib/theme-context"
import { resolveTheme, storedTheme, systemPrefersDark, THEME_STORAGE_KEY, type Theme } from "@/lib/theme-state"

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setThemeState] = useState<Theme>(() => storedTheme(window.localStorage))
  const [systemDark, setSystemDark] = useState(systemPrefersDark)

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)")
    const onChange = (event: MediaQueryListEvent) => setSystemDark(event.matches)
    media.addEventListener("change", onChange)
    return () => media.removeEventListener("change", onChange)
  }, [])

  const resolvedTheme = resolveTheme(theme, systemDark)
  useEffect(() => {
    document.documentElement.classList.toggle("dark", resolvedTheme === "dark")
  }, [resolvedTheme])

  const value = useMemo<ThemeContextValue>(() => ({
    theme,
    resolvedTheme,
    setTheme: (next) => {
      try {
        window.localStorage.setItem(THEME_STORAGE_KEY, next)
      } catch {
        // The selected theme still applies for this session when storage is unavailable.
      }
      setThemeState(next)
    },
  }), [resolvedTheme, theme])

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>
}

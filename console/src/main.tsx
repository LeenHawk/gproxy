import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { App } from "@/app"
import "@/i18n"
import { ThemeProvider } from "@/lib/theme"
import { applyInitialTheme } from "@/lib/theme-state"
import "@/styles/globals.css"

// Before the first paint, so a dark-theme reader does not get a white flash.
applyInitialTheme()

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ThemeProvider><App /></ThemeProvider>
  </StrictMode>,
)

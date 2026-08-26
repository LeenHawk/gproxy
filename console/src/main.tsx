import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { App } from "@/app"
import "@/i18n"
import { ThemeProvider } from "@/lib/theme"
import { applyInitialTheme } from "@/lib/theme-state"
import "@/styles/globals.css"
import "@/styles/public.css"
import "@/styles/public-wire.css"
import "@/styles/public-rail.css"

applyInitialTheme()

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ThemeProvider><App /></ThemeProvider>
  </StrictMode>,
)

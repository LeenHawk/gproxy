import { api } from "@/api/client"

/** `crates/gproxy-host-axum/src/console/custom.rs`. Not generated: a host shape. */
export type ConsoleBundleStatus = { installed: boolean; builtin: boolean; installedAt: number | null; files: number; bytes: number }
export const CONSOLE_BUNDLE_KEY = ["host", "console-bundle"] as const
const PATH = "/admin/api/console-bundle"
export const consoleBundleStatus = () => api<ConsoleBundleStatus>(PATH)
export const uploadConsoleBundle = (file: File) => api<ConsoleBundleStatus>(PATH, { method: "POST", headers: { "content-type": "application/zip" }, body: file })
export const removeConsoleBundle = () => api<ConsoleBundleStatus>(PATH, { method: "DELETE" })
/** Pins this browser to the built-in console, or releases it. See `Console::serve`. */
export const BUILTIN_CONSOLE_URL = "/console/?console=builtin"
export const CUSTOM_CONSOLE_URL = "/console/?console=custom"

import { api } from "@/api/client"
import { shellCall } from "@/api/setup"
import { inShell } from "@/lib/transport"

export type FontStatus = { installed: boolean; downloading: boolean; completed: number; total: number }
export const FONTS_KEY = ["host", "fonts"] as const
export const fontStatus = () => inShell ? shellCall<FontStatus>("desktop_fonts_status") : api<FontStatus>("/admin/api/fonts")
export const downloadFonts = () => inShell ? shellCall<FontStatus>("desktop_fonts_download") : api<FontStatus>("/admin/api/fonts", { method: "POST" })
export const removeFonts = () => inShell ? shellCall<FontStatus>("desktop_fonts_remove") : api<FontStatus>("/admin/api/fonts", { method: "DELETE" })

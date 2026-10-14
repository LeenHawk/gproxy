import { convertFileSrc } from "@tauri-apps/api/core"
import { inShell } from "@/lib/transport"

/** The stylesheet is empty until the host has installed a user-requested pack. */
export function loadFonts() {
  const source = document.querySelector<HTMLMetaElement>('meta[name="gproxy-fonts"]')?.content
  if (!source) return
  document.getElementById("gproxy-font-stylesheet")?.remove()
  const link = document.createElement("link")
  link.id = "gproxy-font-stylesheet"
  link.rel = "stylesheet"
  const url = new URL(source, inShell ? convertFileSrc("", "gproxy-fonts") : location.origin)
  url.searchParams.set("revision", String(Date.now()))
  link.href = url.href
  document.head.append(link)
}

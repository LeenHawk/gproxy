import { convertFileSrc, isTauri } from "@tauri-apps/api/core"

/** The native host caches font files; Edge points this metadata at the CDN. */
export function loadFonts() {
  const source = document.querySelector<HTMLMetaElement>('meta[name="gproxy-fonts"]')?.content
  if (!source) return
  const link = document.createElement("link")
  link.rel = "stylesheet"
  // convertFileSrc encodes a whole filesystem path; use only its platform
  // origin so relative URLs inside the stylesheet keep their directory.
  link.href = isTauri() && import.meta.env.VITE_GPROXY_BUNDLED_FONTS !== "1"
    ? new URL(source, convertFileSrc("", "gproxy-fonts")).href : source
  document.head.append(link)
}

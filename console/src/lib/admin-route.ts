import { useSyncExternalStore } from "react"

export type AdminRoute = "overview" | "providers" | "routes" | "rules" | "keys" | "usage" | "logs" | "channels" | "pricing" | "settings"
export type AdminLocation = { route: AdminRoute; segments: Array<string> }

const routes = new Set<AdminRoute>(["overview", "providers", "routes", "rules", "keys", "usage", "logs", "channels", "pricing", "settings"])
const serverLocation: AdminLocation = { route: "overview", segments: [] }
let cachedPath = ""
let cachedLocation = serverLocation

function readLocation(): AdminLocation {
  const pathname = window.location.pathname
  if (pathname === cachedPath) return cachedLocation
  const parts = pathname.split("/").filter(Boolean)
  cachedPath = pathname
  if (parts[0] !== "admin") {
    cachedLocation = serverLocation
    return cachedLocation
  }
  const candidate = parts[1] as AdminRoute | undefined
  cachedLocation = !candidate || !routes.has(candidate)
    ? serverLocation
    : { route: candidate, segments: parts.slice(2).map(decodeURIComponent) }
  return cachedLocation
}

function subscribe(listener: () => void) {
  window.addEventListener("popstate", listener)
  return () => window.removeEventListener("popstate", listener)
}

export function useAdminLocation() {
  return useSyncExternalStore(subscribe, readLocation, () => serverLocation)
}

export function adminPath(route: AdminRoute) {
  return route === "overview" ? "/admin" : `/admin/${route}`
}

export function navigateAdminPath(path: string, replace = false) {
  if (replace) window.history.replaceState(null, "", path)
  else window.history.pushState(null, "", path)
  window.dispatchEvent(new PopStateEvent("popstate"))
}

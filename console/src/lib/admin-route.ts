import { useSyncExternalStore } from "react"

export type AdminRoute = "overview" | "providers" | "routes" | "keys" | "usage" | "logs" | "channels" | "pricing" | "settings"
export type AdminLocation = { route: AdminRoute; segments: Array<string> }

const routes = new Set<AdminRoute>(["overview", "providers", "routes", "keys", "usage", "logs", "channels", "pricing", "settings"])

function readLocation(): AdminLocation {
  const parts = window.location.pathname.split("/").filter(Boolean)
  if (parts[0] !== "admin") return { route: "overview", segments: [] }
  const candidate = parts[1] as AdminRoute | undefined
  if (!candidate || !routes.has(candidate)) return { route: "overview", segments: [] }
  return { route: candidate, segments: parts.slice(2).map(decodeURIComponent) }
}

function subscribe(listener: () => void) {
  window.addEventListener("popstate", listener)
  return () => window.removeEventListener("popstate", listener)
}

export function useAdminLocation() {
  return useSyncExternalStore(subscribe, readLocation, (): AdminLocation => ({ route: "overview", segments: [] }))
}

export function adminPath(route: AdminRoute) {
  return route === "overview" ? "/admin" : `/admin/${route}`
}

export function navigateAdminPath(path: string, replace = false) {
  if (replace) window.history.replaceState(null, "", path)
  else window.history.pushState(null, "", path)
  window.dispatchEvent(new PopStateEvent("popstate"))
}

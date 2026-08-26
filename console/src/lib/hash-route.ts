import { useSyncExternalStore } from "react"

export type AdminRoute = "overview" | "providers" | "routes" | "keys" | "usage" | "logs" | "channels"

const routes = new Set<AdminRoute>(["overview", "providers", "routes", "keys", "usage", "logs", "channels"])

function readRoute(): AdminRoute {
  const value = window.location.hash.replace(/^#\/?/, "") as AdminRoute
  return routes.has(value) ? value : "overview"
}

function subscribe(listener: () => void) {
  window.addEventListener("hashchange", listener)
  return () => window.removeEventListener("hashchange", listener)
}

export function useAdminRoute() {
  return useSyncExternalStore(subscribe, readRoute, (): AdminRoute => "overview")
}

export function navigate(route: AdminRoute) {
  window.location.hash = `/${route}`
}

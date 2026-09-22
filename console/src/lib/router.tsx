//! The console's router: one history entry per page, and nothing else.
//!
//! A dependency would buy loaders, nested layouts and a data router; this
//! application has one layout and gets its data from TanStack Query, so what
//! is left is "which path is this, and how do I go to another one". That is
//! twenty lines, and it keeps the bundle honest.
//!
//! Every route is written **without** the mount prefix.
//! `gproxy-host-axum/src/console.rs` serves the bundle under `/console` and
//! falls back to `index.html` for any extensionless path below it, so the
//! browser's location is `/console/keys` while this module's routes say
//! `/keys`. [`BASE`] is the one place that knows the difference.

import { useCallback, useEffect, useMemo, useSyncExternalStore, type MouseEvent, type ReactNode } from "react"

/** Where the host mounts the bundle. Vite's `base` is the same string. */
export const BASE = "/console"

const listeners = new Set<() => void>()

function subscribe(listener: () => void) {
  listeners.add(listener)
  window.addEventListener("popstate", listener)
  return () => {
    listeners.delete(listener)
    window.removeEventListener("popstate", listener)
  }
}

function snapshot() {
  return window.location.pathname
}

/** The mount-relative path, always with a leading slash and never trailing one. */
export function routeOf(pathname: string) {
  const rest = pathname.startsWith(BASE) ? pathname.slice(BASE.length) : pathname
  const trimmed = rest.replace(/\/+$/, "")
  return trimmed.startsWith("/") ? trimmed : `/${trimmed}`
}

/** An absolute browser URL for a mount-relative route. */
export function href(route: string) {
  return `${BASE}${route === "/" ? "" : route}` || "/"
}

export function navigate(route: string, options?: { replace?: boolean }) {
  const target = href(route)
  if (options?.replace) window.history.replaceState(null, "", target)
  else window.history.pushState(null, "", target)
  for (const listener of listeners) listener()
}

/** The current mount-relative route. */
export function useRoute() {
  const pathname = useSyncExternalStore(subscribe, snapshot, snapshot)
  return useMemo(() => routeOf(pathname), [pathname])
}

/** Scroll to the top whenever the route changes, as a document navigation would. */
export function useScrollReset(route: string) {
  useEffect(() => {
    window.scrollTo({ top: 0 })
  }, [route])
}

export function useNavigate() {
  return useCallback((route: string, options?: { replace?: boolean }) => navigate(route, options), [])
}

/**
 * An anchor that stays in the application.
 *
 * It is a real `<a href>` so the browser's own affordances — middle click,
 * "open in new tab", the status bar — keep working; only an unmodified left
 * click is intercepted.
 */
export function Link({ to, className, children, onClick, "aria-current": ariaCurrent }: {
  to: string
  className?: string
  children: ReactNode
  onClick?: () => void
  "aria-current"?: "page"
}) {
  const handle = (event: MouseEvent<HTMLAnchorElement>) => {
    if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return
    event.preventDefault()
    onClick?.()
    navigate(to)
  }
  return <a href={href(to)} className={className} onClick={handle} aria-current={ariaCurrent}>{children}</a>
}

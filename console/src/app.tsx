import { lazy, Suspense } from "react"
import { PublicPage } from "@/pages/public"
import { QueryState } from "@/components/query-state"

const AdminSurface = lazy(() => import("@/surfaces/admin-surface").then((module) => ({ default: module.AdminSurface })))
const PortalSurface = lazy(() => import("@/surfaces/portal-surface").then((module) => ({ default: module.PortalSurface })))

type Surface = "public" | "portal" | "admin"

function surfaceForPath(pathname: string): Surface {
  if (pathname === "/admin" || pathname.startsWith("/admin/")) return "admin"
  if (pathname === "/portal" || pathname.startsWith("/portal/")) return "portal"
  return "public"
}

function SurfaceLoading() {
  return <main className="mx-auto max-w-2xl px-5 py-16"><QueryState loading error="">{null}</QueryState></main>
}

export function App() {
  const surface = surfaceForPath(window.location.pathname)
  if (surface === "public") return <PublicPage />
  if (surface === "portal") return <Suspense fallback={<SurfaceLoading />}><PortalSurface /></Suspense>
  return <Suspense fallback={<SurfaceLoading />}><AdminSurface /></Suspense>
}

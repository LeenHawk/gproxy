import type { ReactNode } from "react"
import { useTranslation } from "react-i18next"
import { ActivityIcon, BookOpenIcon, CableIcon, ChartNoAxesCombinedIcon, CircleDollarSignIcon, KeyRoundIcon, LogOutIcon, PanelLeftCloseIcon, PanelLeftOpenIcon, RouteIcon, SettingsIcon, TypeIcon, WorkflowIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Separator } from "@/components/ui/separator"
import { AnnouncementFeed } from "@/components/announcement-feed"
import { LocaleControls } from "@/components/locale-controls"
import { SidebarResizeHandle } from "@/components/sidebar-resize-handle"
import { useSidebarPreferences } from "@/components/use-sidebar-preferences"
import { adminPath, navigateAdminPath, type AdminRoute } from "@/lib/admin-route"
import { cn } from "@/lib/utils"

const items: Array<{ route: AdminRoute; icon: typeof ActivityIcon }> = [
  { route: "overview", icon: ActivityIcon },
  { route: "providers", icon: CableIcon },
  { route: "routes", icon: RouteIcon },
  { route: "rules", icon: WorkflowIcon },
  { route: "keys", icon: KeyRoundIcon },
  { route: "usage", icon: ChartNoAxesCombinedIcon },
  { route: "pricing", icon: CircleDollarSignIcon },
  { route: "tokenizers", icon: TypeIcon },
  { route: "settings", icon: SettingsIcon },
]

export function AppShell({ route, username, children, onLogout }: { route: AdminRoute; username: string; children: ReactNode; onLogout: () => void }) {
  const { t } = useTranslation()
  const sidebar = useSidebarPreferences()
  return (
    <div className="min-h-screen lg:grid" style={{ gridTemplateColumns: `${sidebar.collapsed ? 64 : sidebar.width}px minmax(0, 1fr)` }}>
      <aside className="relative border-b bg-card lg:sticky lg:top-0 lg:h-screen lg:border-r lg:border-b-0">
        <div className="flex h-full flex-col">
          <header className="flex items-center justify-between gap-2 px-3 py-4">
            <div className={cn("flex min-w-0 items-center gap-2", sidebar.collapsed && "lg:sr-only")}>
              {/* Brand mark: the GPROXY globe, the same asset the favicon serves. */}
              <img src="/favicon-96x96.png" className="size-7 shrink-0 rounded" width={28} height={28} alt="" />
              <div className="min-w-0">
                <p className="text-base font-bold tracking-wide">{t("common.product")}</p>
                <p className="text-xs text-muted-foreground">{t("common.console")}</p>
              </div>
            </div>
            <div className="flex items-center gap-2 lg:hidden">
              <span className="max-w-24 truncate font-mono text-xs text-muted-foreground">{username}</span>
              <LocaleControls />
              <Button size="icon-sm" variant="ghost" aria-label={t("auth.logout")} onClick={onLogout}><LogOutIcon /></Button>
            </div>
            <Button className="hidden lg:inline-flex" size="icon-sm" variant="ghost" aria-label={t(sidebar.collapsed ? "nav.open" : "nav.close")} onClick={sidebar.toggle}>{sidebar.collapsed ? <PanelLeftOpenIcon aria-hidden /> : <PanelLeftCloseIcon aria-hidden />}</Button>
          </header>
          <Separator />
          <nav className="flex gap-1 overflow-x-auto overflow-y-hidden p-2 lg:flex-1 lg:flex-col" aria-label={t("nav.label")}>
            {items.map(({ route: itemRoute, icon: Icon }) => (
              <Button key={itemRoute} variant={route === itemRoute ? "secondary" : "ghost"} aria-current={route === itemRoute ? "page" : undefined} aria-label={t(`nav.${itemRoute}`)} className={cn("h-9 justify-start gap-3 px-3", sidebar.collapsed && "lg:justify-center", route === itemRoute && "font-medium")} onClick={() => navigateAdminPath(adminPath(itemRoute))}>
                <Icon className="size-4 shrink-0" aria-hidden />
                <span className={cn(sidebar.collapsed && "lg:sr-only")}>{t(`nav.${itemRoute}`)}</span>
              </Button>
            ))}
          </nav>
          <div className="hidden flex-col gap-2 border-t p-3 lg:flex">
            <div className={cn("flex items-center justify-between gap-1", sidebar.collapsed && "flex-col")}>
              <LocaleControls />
              <Button asChild size="icon-sm" variant="ghost"><a href="https://github.com/LeenHawk/gproxy/blob/3.0/design/architecture.md" target="_blank" rel="noreferrer" aria-label={t("common.documentation")}><BookOpenIcon aria-hidden /></a></Button>
            </div>
            <div className="flex items-center justify-between gap-2">
              <span className={cn("truncate font-mono text-xs text-muted-foreground", sidebar.collapsed && "sr-only")}>{username}</span>
              <Button size="icon-sm" variant="ghost" aria-label={t("auth.logout")} onClick={onLogout}><LogOutIcon /></Button>
            </div>
            <p className={cn("font-mono text-[0.65rem] text-muted-foreground", sidebar.collapsed && "sr-only")}>{t("common.buildIdentity", buildIdentity())}</p>
          </div>
        </div>
        {!sidebar.collapsed ? <SidebarResizeHandle label={t("nav.resize")} width={sidebar.width} minWidth={sidebar.minWidth} maxWidth={sidebar.maxWidth} onWidth={sidebar.setWidth} onReset={sidebar.resetWidth} /> : null}
      </aside>
      <main className="min-w-0 px-4 py-6 sm:px-6 lg:px-8 lg:py-8"><AnnouncementFeed />{children}</main>
    </div>
  )
}

function buildIdentity() {
  const build = window.__GPROXY_BUILD_INFO__
  return build
    ? { version: build.version, channel: build.channel, hash: build.buildHash.slice(0, 12), kind: build.installationKind }
    : { version: __GPROXY_VERSION__, channel: "development", hash: __GPROXY_BUILD_HASH__, kind: "source" }
}

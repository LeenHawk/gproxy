import type { ReactNode } from "react"
import { useTranslation } from "react-i18next"
import { ActivityIcon, CableIcon, KeyRoundIcon, LogsIcon, LogOutIcon, NetworkIcon, RouteIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Separator } from "@/components/ui/separator"
import { LocaleSwitcher } from "@/components/locale-switcher"
import { navigate, type AdminRoute } from "@/lib/hash-route"
import { cn } from "@/lib/utils"

const items: Array<{ route: AdminRoute; icon: typeof ActivityIcon }> = [
  { route: "overview", icon: ActivityIcon },
  { route: "providers", icon: CableIcon },
  { route: "routes", icon: RouteIcon },
  { route: "keys", icon: KeyRoundIcon },
  { route: "usage", icon: ActivityIcon },
  { route: "logs", icon: LogsIcon },
  { route: "channels", icon: NetworkIcon },
]

export function AppShell({ route, username, children, onLogout }: { route: AdminRoute; username: string; children: ReactNode; onLogout: () => void }) {
  const { t } = useTranslation()
  return (
    <div className="min-h-screen lg:grid lg:grid-cols-[15rem_1fr]">
      <aside className="border-b bg-card lg:sticky lg:top-0 lg:h-screen lg:border-r lg:border-b-0">
        <div className="flex h-full flex-col">
          <header className="flex items-center justify-between gap-4 px-4 py-4 lg:block">
            <div>
              <p className="font-mono text-base font-semibold">{t("common.product")}</p>
              <p className="text-xs text-muted-foreground">{t("common.console")}</p>
            </div>
            <div className="flex items-center gap-2 lg:hidden">
              <span className="max-w-24 truncate font-mono text-xs text-muted-foreground">{username}</span>
              <LocaleSwitcher />
              <Button size="icon-sm" variant="ghost" aria-label={t("auth.logout")} onClick={onLogout}><LogOutIcon /></Button>
            </div>
          </header>
          <Separator />
          <nav className="flex gap-1 overflow-x-auto p-2 lg:flex-1 lg:flex-col" aria-label={t("nav.label")}>
            {items.map(({ route: itemRoute, icon: Icon }) => (
              <Button key={itemRoute} variant={route === itemRoute ? "secondary" : "ghost"} aria-current={route === itemRoute ? "page" : undefined} className={cn("justify-start", route === itemRoute && "font-medium")} onClick={() => navigate(itemRoute)}>
                <Icon data-icon="inline-start" />
                {t(`nav.${itemRoute}`)}
              </Button>
            ))}
          </nav>
          <div className="hidden flex-col gap-3 border-t p-3 lg:flex">
            <LocaleSwitcher />
            <div className="flex items-center justify-between gap-2">
              <span className="truncate font-mono text-xs text-muted-foreground">{username}</span>
              <Button size="icon-sm" variant="ghost" aria-label={t("auth.logout")} onClick={onLogout}><LogOutIcon /></Button>
            </div>
          </div>
        </div>
      </aside>
      <main className="min-w-0 px-4 py-6 sm:px-6 lg:px-8 lg:py-8">{children}</main>
    </div>
  )
}

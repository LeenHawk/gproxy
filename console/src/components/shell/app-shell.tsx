import { useState, type ReactNode } from "react";
import { Menu, PanelLeftClose, PanelLeftOpen } from "lucide-react";
import { useTranslation } from "react-i18next";
import { AreaSwitcher } from "@/components/shell/area-switcher";
import { LocaleControls } from "@/components/locale-controls";
import { NavList, NAV_ITEMS, type NavItem } from "@/components/shell/nav";
import { UserMenu } from "@/components/shell/user-menu";
import { Button } from "@/components/ui/button";
import { Sheet, SheetContent, SheetHeader, SheetTitle, SheetTrigger } from "@/components/ui/sheet";
import { cn } from "@/lib/utils";

type ShellFrom = "/_app" | "/_portal";

const COLLAPSE_KEY = "gproxy.sidebar.collapsed";

/** Sidebar collapse preference, persisted across reloads. Defaults to expanded. */
function useSidebarCollapsed(): [boolean, () => void] {
  const [collapsed, setCollapsed] = useState(() => {
    try {
      return localStorage.getItem(COLLAPSE_KEY) === "1";
    } catch {
      return false;
    }
  });
  const toggle = () =>
    setCollapsed((c) => {
      const next = !c;
      try {
        localStorage.setItem(COLLAPSE_KEY, next ? "1" : "0");
      } catch {
        /* ignore storage failures (private mode, etc.) */
      }
      return next;
    });
  return [collapsed, toggle];
}

function Brand({ compact }: { compact?: boolean }) {
  const { t } = useTranslation();
  return (
    <div className="flex h-14 items-center gap-2 px-4 font-semibold">
      {/* v1 brand mark: the GPROXY globe (shared favicon asset under public/). */}
      <img
        src={`${import.meta.env.BASE_URL}favicon-96x96.png`}
        className="size-7 shrink-0 rounded"
        width={28}
        height={28}
        alt="GPROXY"
      />
      <span className={compact ? "hidden" : "font-bold tracking-wide"}>{t("app.name")}</span>
    </div>
  );
}

export function AppShell({
  children,
  navItems = NAV_ITEMS,
  contextFrom = "/_app",
}: {
  children: ReactNode;
  navItems?: NavItem[];
  contextFrom?: ShellFrom;
}) {
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [collapsed, toggleCollapsed] = useSidebarCollapsed();
  const { t } = useTranslation();
  return (
    <div className="flex min-h-svh">
      {/* md+: collapsible sidebar (icon rail when collapsed, labelled when expanded) */}
      <aside
        className={cn(
          "sticky top-0 hidden h-svh shrink-0 flex-col border-r bg-background transition-[width] duration-200 md:flex",
          collapsed ? "w-14" : "w-60",
        )}
      >
        <Brand compact={collapsed} />
        <div className="flex-1 overflow-y-auto py-2">
          <NavList items={navItems} compact={collapsed} />
        </div>
        <div className="border-t p-2">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={toggleCollapsed}
            aria-label={collapsed ? t("nav.expand") : t("nav.collapse")}
            aria-expanded={!collapsed}
            title={collapsed ? t("nav.expand") : t("nav.collapse")}
            className={cn("w-full text-muted-foreground", collapsed ? "justify-center px-2" : "justify-start gap-2")}
          >
            {collapsed ? (
              <PanelLeftOpen className="size-4 shrink-0" aria-hidden />
            ) : (
              <>
                <PanelLeftClose className="size-4 shrink-0" aria-hidden />
                <span>{t("nav.collapse")}</span>
              </>
            )}
          </Button>
          <div className={cn("pt-2 text-[10px] text-muted-foreground", collapsed ? "text-center" : "px-2")}>
            {collapsed ? `v${__APP_VERSION__}` : `v${__APP_VERSION__} · ${__APP_COMMIT__}`}
          </div>
        </div>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col">
        {/* top bar: mobile gets the drawer trigger; all sizes get controls */}
        <header className="sticky top-0 z-10 flex h-14 items-center gap-2 border-b bg-background/95 px-4 backdrop-blur">
          <Sheet open={drawerOpen} onOpenChange={setDrawerOpen}>
            <SheetTrigger asChild>
              <Button variant="ghost" size="icon" className="md:hidden" aria-label={t("nav.openMenu")}>
                <Menu className="size-5" aria-hidden />
              </Button>
            </SheetTrigger>
            <SheetContent side="left" className="w-64 p-0">
              <SheetHeader className="p-0">
                <SheetTitle className="sr-only">{t("nav.title")}</SheetTitle>
              </SheetHeader>
              <Brand />
              <div className="py-2">
                <NavList items={navItems} onNavigate={() => setDrawerOpen(false)} />
              </div>
            </SheetContent>
          </Sheet>
          <div className="flex-1" />
          <AreaSwitcher contextFrom={contextFrom} />
          <LocaleControls />
          <UserMenu />
        </header>
        <main className="min-w-0 flex-1">{children}</main>
      </div>
    </div>
  );
}

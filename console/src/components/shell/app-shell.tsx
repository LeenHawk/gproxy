//! The shell: one navigation, one header, and the page under it.
//!
//! The sidebar is built from [`sectionsFor`], which is built from the caller's
//! capabilities — that is the whole of v4's answer to v3's two surfaces. An
//! ordinary account sees the self-service section and nothing else; an
//! instance operator sees that section and the administrative groups, in the same
//! shell, without a second application being loaded.

import { toast } from "sonner"
import { useEffect, useState, type ReactNode } from "react"
import { useTranslation } from "react-i18next"
import { ChevronRight, ChevronsUpDown, CircleUserRound, Languages, LogOut, Menu, Moon, Sun } from "lucide-react"
import { useIsMutating, useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { INFO_KEY, instanceInfo } from "@/api/settings"
import { signOut } from "@/api/session"
import { useConsoleContext } from "@/capability/session"
import { sectionsFor, type NavSection } from "@/capability/navigation"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuLabel,
  DropdownMenuRadioGroup, DropdownMenuRadioItem, DropdownMenuSeparator, DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Separator } from "@/components/ui/separator"
import { Sheet, SheetContent, SheetTitle, SheetTrigger } from "@/components/ui/sheet"
import { SUPPORTED_LANGS, setLanguage, type LangCode } from "@/i18n"
import { Link, useRoute } from "@/lib/router"
import { useTheme } from "@/lib/theme-context"
import { cn } from "@/lib/utils"

function ThemeToggle() {
  const { t } = useTranslation()
  const { resolvedTheme, setTheme } = useTheme()
  const next = resolvedTheme === "dark" ? "light" : "dark"
  return (
    <Button variant="ghost" size="icon-sm" aria-label={t(`theme.${next}`)} onClick={() => setTheme(next)}>
      {resolvedTheme === "dark" ? <Sun /> : <Moon />}
    </Button>
  )
}

function LanguageMenu() {
  const { t, i18n } = useTranslation()
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="sm"><Languages data-icon="inline-start" />{t(`language.${i18n.language as LangCode}`)}</Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuRadioGroup value={i18n.language} onValueChange={(value) => void setLanguage(value as LangCode)}>
          {SUPPORTED_LANGS.map((code) => (
            <DropdownMenuRadioItem key={code} value={code}>{t(`language.${code}`)}</DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

function AccountMenu() {
  const { t } = useTranslation()
  const context = useConsoleContext()
  const client = useQueryClient()
  // Reset rather than clear: clearing empties the cache but leaves the
  // mounted session query holding its last answer, which keeps the shell on
  // screen after the cookie is already gone. Resetting drops every cached
  // answer *and* re-asks for the session, which is what renders the sign-in
  // page. `onSettled` rather than `onSuccess`, because a sign-out whose
  // request failed still has to end the session on this side.
  const busy = useIsMutating() > 0
  const end = useMutation({ mutationFn: signOut, onSettled: () => client.resetQueries() })
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" className="w-full justify-start gap-3">
          <CircleUserRound data-icon="inline-start" />
          <span className="min-w-0 flex-1 truncate text-left">{context.userName}</span>
          <ChevronsUpDown data-icon="inline-end" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent side="top" align="start" className="w-56">
        <DropdownMenuLabel className="font-normal">
          <span className="block truncate text-sm">{context.userName}</span>
          <span className="block text-xs text-muted-foreground">{t(`scope.${context.scope?.kind ?? "none"}`)}</span>
        </DropdownMenuLabel>
        <DropdownMenuSeparator />
        {context.scopes.length > 1 ? <DropdownMenuRadioGroup value={context.scope?.selector ?? ""} onValueChange={value => { void context.switchScope?.(value).catch(error => { toast.error(String(error)) }) }}>
          {context.scopes.map(scope => <DropdownMenuRadioItem key={scope.selector} value={scope.selector} disabled={busy}>{scope.name ?? t(`scope.${scope.kind}`)}</DropdownMenuRadioItem>)}
        </DropdownMenuRadioGroup> : null}
        <DropdownMenuGroup>
          <DropdownMenuItem disabled={end.isPending} onSelect={() => end.mutate()}>
            <LogOut /> {t("actions.signOut")}
          </DropdownMenuItem>
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

function Brand({ onNavigate }: { onNavigate?: () => void }) {
  const info = useQuery({ queryKey: INFO_KEY, queryFn: instanceInfo })
  const name = info.data?.instanceName === "default" ? "GPROXY" : info.data?.instanceName ?? "GPROXY"
  useEffect(() => { document.title = name }, [name])
  return (
    <Link to="/" onClick={onNavigate} className="flex shrink-0 items-center gap-2.5 font-semibold tracking-tight">
      <img src={`${import.meta.env.BASE_URL}favicon-96x96.png`} alt="" className="size-9" />
      <span className="max-w-36 truncate" title={name}>{name}</span>
    </Link>
  )
}

function Navigation({ sections, route, onNavigate }: {
  sections: Array<NavSection>
  route: string
  onNavigate?: () => void
}) {
  const { t } = useTranslation()
  return (
    <nav className="flex flex-col gap-3" aria-label={t("shell.navigation")}>
      {sections.map((section) => {
        const current = section.items.some((item) => item.route === route)
        const SectionIcon = section.icon
        if (section.standalone || section.id === "rules") return <Link key={section.id} to={section.items[0].route} onClick={onNavigate} aria-current={current ? "page" : undefined} className={cn("flex items-center gap-3 rounded-lg px-3 py-2.5 text-sm font-medium hover:bg-sidebar-accent/60", current ? "bg-sidebar-accent text-sidebar-foreground" : "text-muted-foreground")}><SectionIcon className="size-4.5 shrink-0" aria-hidden /><span>{t(`nav.${section.items[0].id}`)}</span></Link>
        return (
          <details key={section.id} open={current} className="group/section">
            <summary className={cn(
              "flex cursor-pointer list-none items-center gap-3 rounded-lg px-3 py-2.5 text-sm font-medium outline-none transition-colors hover:bg-sidebar-accent/60 focus-visible:ring-2 focus-visible:ring-ring [&::-webkit-details-marker]:hidden",
              current ? "text-sidebar-foreground" : "text-muted-foreground",
            )}>
              <SectionIcon className="size-4.5 shrink-0" strokeWidth={1.75} aria-hidden="true" />
              <span className="flex-1">{t(`section.${section.id}`)}</span>
              <ChevronRight className="size-3.5 shrink-0 transition-transform group-open/section:rotate-90 motion-reduce:transition-none" aria-hidden="true" />
            </summary>
            <ul className="mt-1 ml-5 flex flex-col gap-1 border-l border-sidebar-border pb-1 pl-3">
              {section.items.map((item) => {
                const Icon = item.icon
                const active = route === item.route
                return (
                  <li key={item.id}>
                    <Link
                      to={item.route}
                      onClick={onNavigate}
                      aria-current={active ? "page" : undefined}
                      className={cn(
                        "flex min-h-9 items-center gap-2.5 rounded-lg px-2.5 py-2 text-sm outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring",
                        active
                          ? "bg-sidebar-accent font-medium text-sidebar-primary"
                          : "text-muted-foreground hover:bg-sidebar-accent/60 hover:text-sidebar-foreground",
                      )}
                    >
                      <Icon className="size-4 shrink-0" strokeWidth={1.75} aria-hidden="true" />
                      <span className="truncate">{item.label ?? t(`nav.${item.id}`)}</span>
                    </Link>
                  </li>
                )
              })}
            </ul>
          </details>
        )
      })}
    </nav>
  )
}

function Sidebar({ sections, route, onNavigate }: {
  sections: Array<NavSection>
  route: string
  onNavigate?: () => void
}) {
  const info = useQuery({ queryKey: INFO_KEY, queryFn: instanceInfo })
  return (
    <div className="flex h-full min-h-0 flex-col bg-sidebar text-sidebar-foreground">
      <div className="flex h-16 shrink-0 items-center px-5"><Brand onNavigate={onNavigate} /></div>
      <Separator />
      <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain px-3 py-4">
        <Navigation sections={sections} route={route} onNavigate={onNavigate} />
      </div>
      <Separator />
      <div className="shrink-0 p-3">
        <AccountMenu />
        {info.data ? <div className="mt-2 flex justify-between px-3 text-xs text-muted-foreground"><span>v{info.data.version}</span><code title={info.data.hash}>{info.data.hash.slice(0, 9)}</code></div> : null}
      </div>
    </div>
  )
}

export function AppShell({ children }: { children: ReactNode }) {
  const { t } = useTranslation()
  const context = useConsoleContext()
  const route = useRoute()
  const navRoute = route.startsWith("/providers/") ? "/providers" : route === "/settings/transfer" ? "/settings" : route
  const sections = sectionsFor(context)
  const section = sections.find((group) => group.items.some((item) => item.route === navRoute))
  const item = section?.items.find((item) => item.route === navRoute)
  const ActiveIcon = item?.icon
  const [open, setOpen] = useState(false)

  return (
    <div className="min-h-dvh bg-background text-foreground lg:grid lg:grid-cols-[248px_minmax(0,1fr)]">
      <aside className="sticky top-0 hidden h-dvh border-r border-sidebar-border lg:block">
        <Sidebar sections={sections} route={navRoute} />
      </aside>
      <div className="min-w-0">
        <header className="sticky top-0 z-30 flex h-16 items-center gap-2 border-b border-border bg-background/95 px-4 backdrop-blur lg:px-8">
          <Sheet open={open} onOpenChange={setOpen}>
            <SheetTrigger asChild>
              <Button variant="ghost" size="icon" className="lg:hidden" aria-label={t("shell.navigation")}>
                <Menu />
              </Button>
            </SheetTrigger>
            <SheetContent side="left" closeLabel={t("actions.close")} aria-describedby={undefined} className="gap-0 data-[side=left]:w-72 data-[side=left]:sm:max-w-72">
              <SheetTitle className="sr-only">{t("shell.navigation")}</SheetTitle>
              <Sidebar sections={sections} route={navRoute} onNavigate={() => setOpen(false)} />
            </SheetContent>
          </Sheet>
          <div className="lg:hidden"><Brand /></div>
          {section && item && ActiveIcon ? (
            <div className="hidden min-w-0 items-center gap-2.5 text-sm lg:flex">
              {!section.standalone ? <><span className="text-muted-foreground">{t(`section.${section.id}`)}</span>
              <ChevronRight className="size-3.5 text-muted-foreground" aria-hidden="true" /></> : null}
              <ActiveIcon className="size-4 shrink-0" strokeWidth={1.75} aria-hidden="true" />
              <span className="truncate font-medium">{item.label ?? t(`nav.${item.id}`)}</span>
            </div>
          ) : null}
          <div className="ml-auto flex shrink-0 items-center gap-1">
            <LanguageMenu />
            <ThemeToggle />
          </div>
        </header>
        <main className="mx-auto w-full min-w-0 max-w-[1400px] px-4 py-6 lg:px-8">
          {!context.scope && context.scopes.length ? <div className="mb-6 flex flex-col gap-2"><p>{t("management.chooseScope")}</p><Select onValueChange={value => { void context.switchScope?.(value).catch(error => toast.error(String(error))) }}><SelectTrigger aria-label={t("management.chooseScope")}><SelectValue placeholder={t("management.chooseScope")} /></SelectTrigger><SelectContent><SelectGroup>{context.scopes.map(scope => <SelectItem key={scope.selector} value={scope.selector}>{scope.name ?? t(`scope.${scope.kind}`)}</SelectItem>)}</SelectGroup></SelectContent></Select></div> : null}
          {children}
        </main>
      </div>
    </div>
  )
}

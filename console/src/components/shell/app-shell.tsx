//! The shell: one navigation, one header, and the page under it.
//!
//! The sidebar is built from [`sectionsFor`], which is built from the caller's
//! capabilities — that is the whole of v4's answer to v3's two surfaces. An
//! ordinary account sees the self-service section and nothing else; an
//! instance operator sees that section *and* the identity one, in the same
//! shell, without a second application being loaded.

import { useState, type ReactNode } from "react"
import { useTranslation } from "react-i18next"
import { LogOut, Menu, Moon, Sun, X } from "lucide-react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { signOut } from "@/api/session"
import { useConsoleContext } from "@/capability/session"
import { sectionsFor } from "@/capability/navigation"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuLabel,
  DropdownMenuRadioGroup, DropdownMenuRadioItem, DropdownMenuSeparator, DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
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
        <Button variant="ghost" size="sm">{t(`language.${i18n.language as LangCode}`)}</Button>
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
  const end = useMutation({ mutationFn: signOut, onSettled: () => client.resetQueries() })
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="sm" className="max-w-40 truncate">{context.userName}</Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-56">
        <DropdownMenuLabel className="font-normal">
          <span className="block truncate text-sm">{context.userName}</span>
          <span className="block text-xs text-muted-foreground">{t(`scope.${context.scope?.kind ?? "none"}`)}</span>
        </DropdownMenuLabel>
        <DropdownMenuSeparator />
        <DropdownMenuItem disabled={end.isPending} onSelect={() => end.mutate()}>
          <LogOut /> {t("actions.signOut")}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

function Navigation({ onNavigate }: { onNavigate?: () => void }) {
  const { t } = useTranslation()
  const context = useConsoleContext()
  const route = useRoute()
  return (
    <nav className="space-y-6" aria-label={t("shell.navigation")}>
      {sectionsFor(context).map((section) => (
        <div key={section.id} className="space-y-1">
          <p className="px-3 text-xs font-medium tracking-wide text-muted-foreground uppercase">
            {t(`section.${section.id}`)}
          </p>
          {section.items.map((item) => (
            <Link
              key={item.id}
              to={item.route}
              onClick={onNavigate}
              className={cn(
                // Taller below `lg`: a 28px row is a comfortable menu item
                // for a cursor and a miss for a thumb, and this drawer is the
                // only way to navigate on a phone.
                "block rounded-md px-3 py-2 text-sm transition-colors lg:py-1.5",
                route === item.route
                  ? "bg-accent font-medium text-accent-foreground"
                  : "text-muted-foreground hover:bg-muted hover:text-foreground",
              )}
            >
              {t(`nav.${item.id}`)}
            </Link>
          ))}
        </div>
      ))}
    </nav>
  )
}

export function AppShell({ children }: { children: ReactNode }) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  return (
    <div className="min-h-dvh bg-background text-foreground">
      <header className="sticky top-0 z-30 flex h-14 items-center gap-2 border-b border-border bg-background/95 px-4 backdrop-blur">
        <Button
          variant="ghost"
          size="icon"
          className="lg:hidden"
          aria-label={t("shell.navigation")}
          aria-expanded={open}
          onClick={() => setOpen((value) => !value)}
        >
          {open ? <X /> : <Menu />}
        </Button>
        <Link to="/" className="font-mono text-sm font-semibold tracking-tight">gproxy</Link>
        <span className="ml-auto flex items-center gap-1">
          <LanguageMenu />
          <ThemeToggle />
          <AccountMenu />
        </span>
      </header>
      <div className="mx-auto flex w-full max-w-[1600px]">
        {/*
          The scrim is what makes the drawer a drawer: it dismisses on a tap
          anywhere outside, which is the gesture a phone user already has. It
          is a button rather than a div so the same dismissal is reachable
          from a keyboard, and it is only mounted while the drawer is open.
        */}
        {open ? (
          <button
            type="button"
            aria-label={t("actions.close")}
            className="fixed inset-x-0 top-14 bottom-0 z-20 bg-foreground/20 lg:hidden"
            onClick={() => setOpen(false)}
          />
        ) : null}
        <aside
          className={cn(
            "w-60 shrink-0 border-r border-border px-2 py-6",
            // Below `lg` the navigation floats *over* the page rather than
            // beside it. As a column it took 240 of a 390px viewport and left
            // the page it had just navigated to 150px to render in, which is
            // not a narrow layout but a broken one.
            "max-lg:fixed max-lg:top-14 max-lg:bottom-0 max-lg:left-0 max-lg:z-30 max-lg:overflow-y-auto max-lg:overscroll-contain max-lg:bg-background",
            open ? "block" : "hidden lg:block",
          )}
        >
          <Navigation onNavigate={() => setOpen(false)} />
        </aside>
        <main className="min-w-0 flex-1 px-4 py-6 lg:px-8">{children}</main>
      </div>
    </div>
  )
}

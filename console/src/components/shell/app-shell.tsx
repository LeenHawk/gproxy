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
                "block rounded-md px-3 py-1.5 text-sm transition-colors",
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
          size="icon-sm"
          className="lg:hidden"
          aria-label={t("shell.navigation")}
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
        <aside
          className={cn(
            "w-60 shrink-0 border-r border-border px-2 py-6",
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

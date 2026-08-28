import type { ReactNode } from "react"
import { useTranslation } from "react-i18next"
import type { PortalContextDto } from "@/generated/PortalContextDto"
import { LocaleControls } from "@/components/locale-controls"
import { Button } from "@/components/ui/button"

export function PortalShell({
  context,
  onLogout,
  children,
}: {
  context: PortalContextDto | null
  onLogout?: () => void
  children: ReactNode
}) {
  const { t } = useTranslation()

  return (
    <div className="min-h-screen bg-background">
      <header className="border-b bg-card">
        <div className="mx-auto flex max-w-7xl flex-wrap items-center justify-between gap-4 px-4 py-4 sm:px-6 lg:px-8">
          <div>
            <a className="font-mono text-base font-semibold text-foreground no-underline" href="/">{t("portal.brand")}</a>
            <p className="text-xs text-muted-foreground">{t("portal.surface")}</p>
          </div>
          <div className="flex items-center gap-3">
            <LocaleControls />
            {context && onLogout ? (
              <>
                <div className="hidden text-right sm:block">
                  <p className="text-sm font-medium">{context.user_name}</p>
                  <p className="font-mono text-xs text-muted-foreground">
                    {t("portal.account.maskedKey", { prefix: context.key_prefix ?? "" })}
                  </p>
                </div>
                <Button variant="outline" onClick={onLogout}>
                  {t("portal.account.logout")}
                </Button>
              </>
            ) : null}
          </div>
        </div>
      </header>
      <main className="mx-auto flex max-w-7xl flex-col gap-6 px-4 py-8 sm:px-6 lg:px-8">
        {children}
      </main>
    </div>
  )
}

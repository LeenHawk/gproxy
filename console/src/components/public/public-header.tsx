import { useTranslation } from "react-i18next"
import { LocaleControls } from "@/components/locale-controls"
import { Button } from "@/components/ui/button"

export function PublicHeader() {
  const { t } = useTranslation()
  return (
    <header className="public-masthead">
      <a className="public-brand public-display" href="/">{t("common.product")}</a>
      <nav className="public-nav" aria-label={t("public.nav.label")}>
        <LocaleControls />
        <Button asChild variant="outline" size="sm"><a href="/admin">{t("public.nav.admin")}</a></Button>
        <Button asChild size="sm"><a href="/portal">{t("public.nav.portal")}</a></Button>
      </nav>
    </header>
  )
}

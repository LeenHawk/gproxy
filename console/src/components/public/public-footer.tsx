import { useTranslation } from "react-i18next"
import { buildIdentity } from "@/lib/build-info"
import { DOCS_URL, REPO_URL } from "@/lib/project-links"

export function PublicFooter() {
  const { t } = useTranslation()
  const build = buildIdentity()
  return (
    <footer className="public-footer">
      <div className="public-footer-meta">
        <span className="public-footer-brand">{t("common.product")}</span>
        <span className="public-machine">{t("public.version", { version: build.version })}</span>
        <span>{t("public.footer.license")}</span>
      </div>
      <nav aria-label={t("public.footer.label")}>
        <a href={REPO_URL} target="_blank" rel="noreferrer">{t("public.footer.source")}</a>
        <a href={DOCS_URL} target="_blank" rel="noreferrer">{t("common.documentation")}</a>
        <a href="/admin">{t("public.nav.admin")}</a>
      </nav>
    </footer>
  )
}

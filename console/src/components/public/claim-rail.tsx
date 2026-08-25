import type { ReactNode } from "react"
import { useTranslation } from "react-i18next"

function Claim({ title, children, featured = false }: { title: string; children: ReactNode; featured?: boolean }) {
  return (
    <article className={featured ? "public-claim public-claim-featured" : "public-claim"}>
      <h3 className="public-display">{title}</h3>
      <p>{children}</p>
    </article>
  )
}

export function ClaimRail() {
  const { t } = useTranslation()
  return (
    <section className="public-claims" aria-labelledby="public-claims-title">
      <div className="public-claims-heading">
        <h2 id="public-claims-title" className="public-display">{t("public.claims.title")}</h2>
      </div>
      <div className="public-convergence" role="img" aria-label={t("public.claims.convergenceLabel")}>
        <div className="public-lanes">
          <span>{t("public.hero.dialects.openai")}</span>
          <span>{t("public.hero.dialects.claude")}</span>
          <span>{t("public.hero.dialects.gemini")}</span>
        </div>
        <strong className="public-display">{t("public.claims.funnel")}</strong>
      </div>
      <div className="public-claim-rail">
        <div className="public-rail-stop public-rail-stop-lead">
          <Claim title={t("public.claims.compile.title")} featured>{t("public.claims.compile.body")}</Claim>
        </div>
        <div className="public-rail-stop public-rail-pair">
          <Claim title={t("public.claims.pairwise.title")}>{t("public.claims.pairwise.body")}</Claim>
          <Claim title={t("public.claims.unknown.title")}>{t("public.claims.unknown.body")}</Claim>
        </div>
        <div className="public-rail-stop public-rail-stop-cli">
          <Claim title={t("public.claims.cli.title")}>{t("public.claims.cli.body")}</Claim>
        </div>
        <div className="public-rail-stop public-rail-pair public-rail-pair-final">
          <Claim title={t("public.claims.providers.title")}>{t("public.claims.providers.body")}</Claim>
          <Claim title={t("public.claims.runtime.title")}>{t("public.claims.runtime.body")}</Claim>
        </div>
      </div>
    </section>
  )
}

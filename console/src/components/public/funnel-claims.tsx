import { useTranslation } from "react-i18next"

const stages = ["ingress", "classify", "auth", "route", "admit", "transform", "upstream"] as const
const funnel = ["settle", "capture", "telemetry"] as const
const claims = ["pairwise", "unknown", "cli", "providers", "runtime"] as const

export function FunnelClaims() {
  const { t } = useTranslation()
  return (
    <section className="public-funnel" aria-labelledby="public-funnel-title">
      <h2 id="public-funnel-title" className="public-section-title">{t("public.funnel.title")}</h2>
      <p className="public-section-lede">{t("public.funnel.description")}</p>
      <ol className="public-pipeline public-machine" aria-label={t("public.funnel.pipelineLabel")}>
        {stages.map((stage) => (
          <li key={stage}><span className="public-stage">{t(`public.funnel.stages.${stage}`)}</span></li>
        ))}
        <li>
          <ol className="public-pipeline-funnel" aria-label={t("public.funnel.funnelLabel")}>
            {funnel.map((stage) => (
              <li key={stage} className="public-stage">{t(`public.funnel.stages.${stage}`)}</li>
            ))}
          </ol>
        </li>
      </ol>
      <p className="public-funnel-caption public-machine">{t("public.funnel.caption")}</p>
      <div className="public-claims">
        {claims.map((claim) => (
          <article key={claim} className="public-claim">
            <h3>{t(`public.claims.${claim}.title`)}</h3>
            <p>{t(`public.claims.${claim}.body`)}</p>
          </article>
        ))}
      </div>
    </section>
  )
}

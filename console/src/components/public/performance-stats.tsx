import { useTranslation } from "react-i18next"

const stats = ["latency", "throughput", "connections", "routing"] as const

export function PerformanceStats() {
  const { t } = useTranslation()
  return (
    <section className="public-performance" aria-labelledby="public-performance-title">
      <h2 id="public-performance-title" className="public-section-title">{t("public.performance.title")}</h2>
      <p className="public-section-lede">{t("public.performance.description")}</p>
      <dl className="public-stats">
        {stats.map((stat) => (
          <div key={stat} className="public-stat">
            <dt>{t(`public.performance.stats.${stat}.label`)}</dt>
            <dd className="public-stat-value public-display">{t(`public.performance.stats.${stat}.value`)}</dd>
            <dd className="public-stat-body">{t(`public.performance.stats.${stat}.body`)}</dd>
          </div>
        ))}
      </dl>
      <p className="public-performance-caption public-machine">{t("public.performance.caption")}</p>
    </section>
  )
}

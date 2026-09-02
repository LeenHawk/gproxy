import { useTranslation } from "react-i18next"
import { WireDiff } from "@/components/public/wire-diff"

export function PublicHero() {
  const { t } = useTranslation()
  return (
    <section className="public-hero" aria-labelledby="public-title">
      <div className="public-hero-copy">
        <h1 id="public-title" className="public-display">{t("public.hero.title")}</h1>
        <p>{t("public.hero.description")}</p>
      </div>
      <WireDiff />
    </section>
  )
}

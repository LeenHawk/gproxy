import { useState } from "react"
import { useTranslation } from "react-i18next"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"

type Dialect = "openai" | "claude" | "gemini"

export function DialectLab() {
  const { t } = useTranslation()
  const [dialect, setDialect] = useState<Dialect>("claude")
  const endpoint = t(dialect === "openai" ? "public.hero.endpoints.openai" : dialect === "claude" ? "public.hero.endpoints.claude" : "public.hero.endpoints.gemini")
  const wire = t(dialect === "openai" ? "public.hero.wires.openai" : dialect === "claude" ? "public.hero.wires.claude" : "public.hero.wires.gemini")

  return (
    <div className="public-lab" aria-label={t("public.hero.labLabel")}>
      <div className="public-lab-toolbar">
        <p className="public-lab-title public-display">{t("public.hero.labTitle")}</p>
        <ToggleGroup
          type="single"
          variant="outline"
          value={dialect}
          className="public-dialect-switch"
          aria-label={t("public.hero.switchLabel")}
          onValueChange={(value) => { if (value) setDialect(value as Dialect) }}
        >
          <ToggleGroupItem value="openai">{t("public.hero.dialects.openai")}</ToggleGroupItem>
          <ToggleGroupItem value="claude">{t("public.hero.dialects.claude")}</ToggleGroupItem>
          <ToggleGroupItem value="gemini">{t("public.hero.dialects.gemini")}</ToggleGroupItem>
        </ToggleGroup>
      </div>
      <div className="public-wire-grid">
        <figure className="public-wire-pane">
          <figcaption><span>{t("public.hero.clientLabel")}</span><code>{t("public.hero.endpoints.openai")}</code></figcaption>
          <pre><code>{t("public.hero.clientCode")}</code></pre>
        </figure>
        <div className="public-wire-seam" aria-hidden="true"><span /></div>
        <figure className="public-wire-pane public-wire-pane-output">
          <figcaption><span>{t("public.hero.providerLabel")}</span><code>{endpoint}</code></figcaption>
          <pre key={dialect} className="public-wire-code" aria-live="polite" aria-atomic="true"><code>{wire}</code></pre>
        </figure>
      </div>
    </div>
  )
}

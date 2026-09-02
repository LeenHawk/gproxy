import { useMemo, useState, type CSSProperties } from "react"
import { useTranslation } from "react-i18next"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { lineDiff, type DiffLine } from "@/components/public/line-diff"

type Dialect = "openai" | "claude" | "gemini"

const dialects: Array<Dialect> = ["openai", "claude", "gemini"]

function Lines({ lines }: { lines: Array<DiffLine> }) {
  let changed = 0
  const rows = lines.map((line) => ({ ...line, order: line.kind === "changed" ? changed++ : 0 }))
  return (
    <pre className="public-wire-lines public-machine">
      <code>
        {rows.map((line, index) => (
          <span key={index} className="public-wire-line" data-kind={line.kind} style={{ "--i": line.order } as CSSProperties}>
            {line.text || " "}
          </span>
        ))}
      </code>
    </pre>
  )
}

export function WireDiff() {
  const { t } = useTranslation()
  const [dialect, setDialect] = useState<Dialect>("claude")
  const client = t("public.hero.clientCode")
  const wire = t(`public.hero.wires.${dialect}`)
  const clientEndpoint = t("public.hero.endpoints.openai")
  const endpoint = t(`public.hero.endpoints.${dialect}`)
  const diff = useMemo(() => lineDiff(client, wire), [client, wire])
  const endpointKind = endpoint === clientEndpoint ? "same" : "changed"
  const kind = diff.rewritten === 0 && endpointKind === "same" ? "same" : "changed"

  return (
    <figure className="public-wire" aria-label={t("public.hero.labLabel")}>
      <div className="public-wire-grid">
        <div className="public-wire-pane">
          <div className="public-wire-head">
            <span className="public-wire-role">{t("public.hero.clientLabel")}</span>
            <span className="public-wire-dialect public-machine">{t("public.hero.dialects.openai")}</span>
          </div>
          <code className="public-wire-endpoint public-machine">{clientEndpoint}</code>
          <Lines lines={diff.left} />
        </div>
        <div className="public-wire-pane public-wire-pane-provider">
          <div className="public-wire-head">
            <span className="public-wire-role">{t("public.hero.providerLabel")}</span>
            <ToggleGroup
              type="single"
              variant="outline"
              size="sm"
              value={dialect}
              className="public-dialect-switch"
              aria-label={t("public.hero.switchLabel")}
              onValueChange={(value) => { if (value) setDialect(value as Dialect) }}
            >
              {dialects.map((value) => (
                <ToggleGroupItem key={value} value={value}>{t(`public.hero.dialects.${value}`)}</ToggleGroupItem>
              ))}
            </ToggleGroup>
          </div>
          <code key={`endpoint-${dialect}`} className="public-wire-endpoint public-machine" data-kind={endpointKind}>{endpoint}</code>
          <Lines key={`lines-${dialect}`} lines={diff.right} />
        </div>
      </div>
      <figcaption className="public-wire-foot public-machine" data-kind={kind} aria-live="polite">
        {kind === "same"
          ? t("public.hero.passthrough")
          : t("public.hero.summary", { rewritten: diff.rewritten, kept: diff.kept })}
      </figcaption>
    </figure>
  )
}

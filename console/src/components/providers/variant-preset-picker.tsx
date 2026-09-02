import { useMemo, useState } from "react"
import { useTranslation } from "react-i18next"
import { Button } from "@/components/ui/button"
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { GATEWAY_SOURCE_BY_CHANNEL, VARIANT_PROTOCOL_LABELS, gatewayActionPath, variantGroups, type VariantAction, type VariantProtocol } from "@/components/providers/variant-presets"
import { inferVariantSelection } from "@/components/providers/variant-presets/selection"

const NONE = "__none__"

const upstreams = (value: string) => [...new Set(value.split(",").map((item) => item.trim()).filter(Boolean))]

function upstreamSuffix(values: Array<string>) {
  const value = values.map((item) => item.replace(/[^a-zA-Z0-9._-]+/g, "-").replace(/^-+|-+$/g, "")).filter(Boolean).join("-")
  return value ? `-via-${value}` : ""
}

export function VariantPresetPicker({ modelId, channel, initialActions, onApply, onCancel }: {
  modelId: string
  channel: string
  initialActions: Array<VariantAction>
  onApply: (actions: Array<VariantAction>, suffix: string) => void
  onCancel: () => void
}) {
  const { t } = useTranslation()
  const [initial] = useState(() => inferVariantSelection(channel, initialActions))
  const [protocol, setProtocol] = useState<VariantProtocol>(initial.protocol)
  const [picks, setPicks] = useState<Record<string, string>>(initial.picks)
  const [upstream, setUpstream] = useState(initial.upstream)
  const groups = variantGroups(protocol, channel)
  const sourceKey = GATEWAY_SOURCE_BY_CHANNEL[channel]?.key
  const upstreamPath = gatewayActionPath(channel)
  const selection = useMemo(() => {
    let suffix = ""
    const actions: Array<VariantAction> = []
    for (const group of groups) {
      const picked = picks[group.key]
      if (!picked || picked === NONE) continue
      const entry = group.entries[Number(picked)]
      if (!entry) continue
      suffix += entry.suffix
      actions.push(...entry.actions)
    }
    const sources = upstreams(upstream)
    if (upstreamPath && sources.length > 0) {
      suffix += upstreamSuffix(sources)
      actions.push({ path: upstreamPath, value: sources })
    }
    const paths = new Set(actions.map((action) => action.path))
    return { suffix, actions: [...actions, ...initial.preserved.filter((action) => !paths.has(action.path))] }
  }, [groups, initial.preserved, picks, upstream, upstreamPath])

  return <div className="grid gap-3 rounded-md border bg-muted/30 p-3">
    <Field>
      <FieldLabel>{t("providers.models.variantPicker.protocol")}</FieldLabel>
      <Select value={protocol} onValueChange={(value) => { setProtocol(value as VariantProtocol); setPicks({}) }}>
        <SelectTrigger><SelectValue /></SelectTrigger>
        <SelectContent>{(Object.keys(VARIANT_PROTOCOL_LABELS) as Array<VariantProtocol>).map((value) => <SelectItem key={value} value={value}>{VARIANT_PROTOCOL_LABELS[value]}</SelectItem>)}</SelectContent>
      </Select>
    </Field>
    {groups.map((group) => <Field key={group.key}>
      <FieldLabel>{group.label}</FieldLabel>
      <Select value={picks[group.key] ?? NONE} onValueChange={(value) => {
        setPicks((current) => ({ ...current, [group.key]: value }))
        if (group.key === sourceKey && value !== NONE) setUpstream("")
      }}>
        <SelectTrigger><SelectValue /></SelectTrigger>
        <SelectContent>
          <SelectItem value={NONE}>{t("providers.models.variantPicker.none")}</SelectItem>
          {group.entries.map((entry, index) => <SelectItem key={`${entry.suffix}-${index}`} value={String(index)}>{entry.suffix} — {entry.label}</SelectItem>)}
        </SelectContent>
      </Select>
    </Field>)}
    {upstreamPath ? <Field>
      <FieldLabel htmlFor="variant-upstreams">{t("providers.models.variantPicker.upstream")}</FieldLabel>
      <Input id="variant-upstreams" className="machine-text text-xs" value={upstream} placeholder={channel === "openrouter" ? "anthropic, google-vertex/us-east5" : "anthropic, bedrock"} onChange={(event) => {
        setUpstream(event.target.value)
        if (event.target.value.trim() && sourceKey) setPicks((current) => ({ ...current, [sourceKey]: NONE }))
      }} />
      <FieldDescription>{t("providers.models.variantPicker.upstreamHint")}</FieldDescription>
    </Field> : null}
    <div className="rounded-md border bg-background p-3 text-xs">
      <p className="text-muted-foreground">{t("providers.models.variantPicker.suggestedName")}</p>
      <p className="machine-text mt-1">{selection.suffix ? `${modelId}${selection.suffix}` : (modelId || "—")}</p>
      {selection.actions.length > 0 ? <div className="mt-3 grid gap-1">
        <p className="text-muted-foreground">{t("providers.models.variantPicker.injects")}</p>
        {selection.actions.map((action, index) => <p key={`${action.path}-${index}`} className="machine-text">{action.path} = {JSON.stringify(action.value)}</p>)}
      </div> : null}
    </div>
    <div className="flex justify-end gap-2">
      <Button type="button" size="sm" variant="ghost" onClick={onCancel}>{t("common.actions.cancel")}</Button>
      <Button type="button" size="sm" disabled={selection.actions.length === 0 && initialActions.length === 0} onClick={() => onApply(selection.actions, selection.suffix)}>{t("providers.models.variantPicker.apply")}</Button>
    </div>
  </div>
}

import { useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import type { QuotaDto, QuotaWrite } from "@/generated/sdk"
import { fromLocalInput, toLocalInput } from "@/lib/format"
import { ErrorNotice } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"

export function QuotaEditor({ original, limit, override, pending, error, onSave, onCancel }: { original?: QuotaDto; limit: boolean; override?: boolean; pending: boolean; error: unknown; onSave: (body: Partial<QuotaWrite>) => void; onCancel: () => void }) {
  const { t } = useTranslation()
  const [name, setName] = useState(original?.windowKey ?? t(limit ? "limits.defaultRule" : "limits.defaultBudget"))
  const [metric, setMetric] = useState(original?.metric ?? "cost")
  const [amount, setAmount] = useState(original?.limitValue ?? "")
  const [period, setPeriod] = useState(original?.period ?? "1m")
  const [seconds, setSeconds] = useState(original?.periodSeconds == null ? "" : String(original.periodSeconds))
  const [anchor, setAnchor] = useState(toLocalInput(original?.anchorAtMs))
  const [pattern, setPattern] = useState(original?.modelPattern ?? "")
  const [enabled, setEnabled] = useState(original?.enabled ?? true)
  const periods = ["5h", "1d", "7d", "1m", "total", "custom"]
  if (!periods.includes(period)) periods.push(period)
  const customPeriod = !["5h", "1d", "7d", "1m", "total"].includes(period)
  function submit(event: FormEvent) {
    event.preventDefault()
    const body: Record<string, unknown> = { windowKey: name.trim(), metric, unit: metric === "requests" ? "count" : "USD", limitValue: amount.trim(), period, periodSeconds: customPeriod ? Number(seconds) : null, anchorAtMs: fromLocalInput(anchor), modelPattern: pattern.trim() || null, enabled }
    if (original && !override) for (const key of Object.keys(body)) if (body[key] === original[key as keyof QuotaDto]) delete body[key]
    onSave(body)
  }
  return <form onSubmit={submit} className="flex flex-col gap-4">
    <h3 className="font-medium">{t(override ? "limits.override" : original ? "limits.edit" : "limits.add")}</h3>
    {error ? <ErrorNotice error={error} /> : null}
    <fieldset disabled={pending}><FieldGroup>
      <Field><FieldLabel htmlFor="limit-name">{t("limits.ruleName")}</FieldLabel><Input id="limit-name" value={name} required readOnly={override} onChange={event => setName(event.target.value)} /></Field>
      {limit ? <Field><FieldLabel htmlFor="limit-metric">{t("limits.measure")}</FieldLabel><Select value={metric} onValueChange={setMetric}><SelectTrigger id="limit-metric"><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{["cost", "requests"].map(value => <SelectItem key={value} value={value}>{t(`management.${value}`)}</SelectItem>)}</SelectGroup></SelectContent></Select></Field> : null}
      <Field><FieldLabel htmlFor="limit-amount">{t(metric === "requests" ? "limits.requestLimit" : "limits.costLimit")}</FieldLabel><Input id="limit-amount" value={amount} inputMode={metric === "requests" ? "numeric" : "decimal"} pattern={metric === "requests" ? "[0-9]+" : "[0-9]+([.][0-9]+)?"} required onChange={event => setAmount(event.target.value)} /></Field>
      <Field><FieldLabel htmlFor="limit-period">{t("fields.period")}</FieldLabel><Select value={period} onValueChange={setPeriod}><SelectTrigger id="limit-period"><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{periods.map(value => <SelectItem key={value} value={value}>{t(`userQuota.periods.${value}`, { defaultValue: value })}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
      <Field><FieldLabel htmlFor="limit-model">{t("limits.appliesTo")}</FieldLabel><Input id="limit-model" value={pattern} placeholder={t("limits.allModels")} onChange={event => setPattern(event.target.value)} /></Field>
      <Field orientation="horizontal"><FieldLabel htmlFor="limit-enabled">{t("fields.enabled")}</FieldLabel><Switch id="limit-enabled" checked={enabled} onCheckedChange={setEnabled} /></Field>
      {period !== "1m" && period !== "total" ? <details open={customPeriod || undefined}><summary className="cursor-pointer text-sm">{t("limits.advanced")}</summary><FieldGroup className="mt-3">
        {customPeriod ? <Field><FieldLabel htmlFor="limit-seconds">{t("fields.periodSeconds")}</FieldLabel><Input id="limit-seconds" type="number" min={1} step={1} required value={seconds} onChange={event => setSeconds(event.target.value)} /></Field> : null}
        <Field><FieldLabel htmlFor="limit-anchor">{t("limits.anchor")}</FieldLabel><Input id="limit-anchor" type="datetime-local" value={anchor} onChange={event => setAnchor(event.target.value)} /></Field>
      </FieldGroup></details> : null}
    </FieldGroup></fieldset>
    <div className="flex justify-end gap-2"><Button type="button" variant="outline" disabled={pending} onClick={onCancel}>{t("actions.cancel")}</Button><Button type="submit" disabled={pending || !name.trim() || !amount.trim()}>{t("actions.save")}</Button></div>
  </form>
}

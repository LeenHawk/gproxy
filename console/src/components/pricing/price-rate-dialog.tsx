import type { ReactElement } from "react"
import type { PriceRateDto } from "@/generated/PriceRateDto"
import type { PriceRateWriteRequest } from "@/generated/PriceRateWriteRequest"
import type { PriceRuleDto } from "@/generated/PriceRuleDto"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { PlusIcon, Trash2Icon } from "lucide-react"
import { useState } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { savePriceRate } from "@/api/control"
import { SearchableSelect } from "@/components/searchable-select"
import { Button } from "@/components/ui/button"
import { Dialog, DialogBody, DialogClose, DialogContent, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"

type Condition = { key: string; value: string }

function conditionRows(value: unknown): Array<Condition> {
  if (value == null || typeof value !== "object" || Array.isArray(value)) return []
  return Object.entries(value as Record<string, unknown>).flatMap(([key, item]) =>
    typeof item === "string" ? [{ key, value: item }] : [])
}

export function PriceRateDialog({ rate, rules, initialRuleId, trigger }: {
  rate?: PriceRateDto
  rules: Array<PriceRuleDto>
  initialRuleId?: number
  trigger: ReactElement
}) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const [open, setOpen] = useState(false)
  const [ruleId, setRuleId] = useState(String(rate?.rule_id ?? initialRuleId ?? rules[0]?.id ?? 0))
  const [metric, setMetric] = useState(rate?.metric ?? "")
  const [unitSize, setUnitSize] = useState(String(rate?.unit_size ?? 1))
  const [price, setPrice] = useState(rate?.price ?? "")
  const [priority, setPriority] = useState(String(rate?.priority ?? 0))
  const [conditions, setConditions] = useState(() => conditionRows(rate?.conditions))
  const mutation = useMutation({
    mutationFn: (value: PriceRateWriteRequest) => savePriceRate(value, rate?.id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["price-rates"] })
      toast.success(t(rate ? "pricing.rates.updated" : "pricing.rates.created"))
      setOpen(false)
    },
    onError: () => toast.error(t("pricing.rates.saveError")),
  })
  const submit = (event: React.FormEvent) => {
    event.preventDefault()
    mutation.mutate({
      rule_id: Number(ruleId),
      metric: metric.trim(),
      unit_size: Number(unitSize),
      price: price.trim(),
      conditions: conditions.length ? Object.fromEntries(conditions.map((row) => [row.key.trim(), row.value.trim()])) : null,
      priority: Number(priority),
    })
  }
  const patch = (index: number, value: Partial<Condition>) => setConditions((rows) =>
    rows.map((row, rowIndex) => rowIndex === index ? { ...row, ...value } : row))
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>{trigger}</DialogTrigger>
      <DialogContent className="sm:max-w-2xl" showCloseButton={false}>
        <form className="flex min-h-0 flex-1 flex-col" onSubmit={submit}>
          <DialogHeader><DialogTitle>{t(rate ? "pricing.rates.edit" : "pricing.rates.add")}</DialogTitle></DialogHeader>
          <DialogBody><FieldGroup>
            <Field><FieldLabel htmlFor="rate-rule">{t("pricing.rates.rule")}</FieldLabel><SearchableSelect id="rate-rule" value={ruleId} options={rules.map((rule) => ({ value: String(rule.id), label: rule.model_pattern }))} placeholder={t("common.none")} searchPlaceholder={t("common.search")} emptyLabel={t("common.none")} ariaLabel={t("pricing.rates.rule")} onChange={setRuleId} /></Field>
            <div className="grid gap-4 sm:grid-cols-2">
              <Field><FieldLabel htmlFor="rate-metric">{t("pricing.rates.metric")}</FieldLabel><Input id="rate-metric" className="font-mono" required value={metric} onChange={(event) => setMetric(event.target.value)} /></Field>
              <Field><FieldLabel htmlFor="rate-price">{t("pricing.rates.price")}</FieldLabel><Input id="rate-price" inputMode="decimal" required value={price} onChange={(event) => setPrice(event.target.value)} /></Field>
              <Field><FieldLabel htmlFor="rate-unit">{t("pricing.rates.unitSize")}</FieldLabel><Input id="rate-unit" type="number" min={1} step={1} required value={unitSize} onChange={(event) => setUnitSize(event.target.value)} /></Field>
              <Field><FieldLabel htmlFor="rate-priority">{t("pricing.rates.priority")}</FieldLabel><Input id="rate-priority" type="number" step={1} required value={priority} onChange={(event) => setPriority(event.target.value)} /></Field>
            </div>
            <Field>
              <div className="flex items-center justify-between gap-2"><FieldLabel>{t("pricing.rates.conditions")}</FieldLabel><Button type="button" size="sm" variant="outline" onClick={() => setConditions((rows) => [...rows, { key: "", value: "" }])}><PlusIcon data-icon="inline-start" />{t("common.actions.add")}</Button></div>
              {conditions.map((row, index) => <div key={index} className="grid gap-2 sm:grid-cols-[1fr_1fr_auto]"><Input aria-label={t("pricing.rates.conditionKey")} value={row.key} onChange={(event) => patch(index, { key: event.target.value })} /><Input aria-label={t("pricing.rates.conditionValue")} value={row.value} onChange={(event) => patch(index, { value: event.target.value })} /><Button type="button" size="icon-sm" variant="ghost" aria-label={t("common.actions.delete")} onClick={() => setConditions((rows) => rows.filter((_, rowIndex) => rowIndex !== index))}><Trash2Icon /></Button></div>)}
            </Field>
          </FieldGroup></DialogBody>
          <DialogFooter><DialogClose asChild><Button type="button" variant="outline">{t("common.actions.cancel")}</Button></DialogClose><Button type="submit" disabled={mutation.isPending}>{t(mutation.isPending ? "common.actions.saving" : "common.actions.save")}</Button></DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

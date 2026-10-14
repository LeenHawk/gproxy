import { useState } from "react"
import { useTranslation } from "react-i18next"
import type { ApiKeyBudgetWrite } from "@/generated/app"
import { RecordDialog, type FormField } from "@/components/record-form"
import { Field, FieldContent, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Separator } from "@/components/ui/separator"
import { Switch } from "@/components/ui/switch"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"

type Props = { open: boolean; onOpenChange: (open: boolean) => void; title: string; fields: readonly FormField[]; canSetBudget: boolean; onSubmit: (body: Record<string, unknown>) => void; pending: boolean; error: unknown; defaults?: Record<string, unknown> }
export function KeyCreateDialog(props: Props) { return props.open ? <KeyCreateForm {...props} /> : null }
function KeyCreateForm(props: Props) {
  const { t } = useTranslation()
  const [limited, setLimited] = useState(false)
  const [amount, setAmount] = useState("")
  const [period, setPeriod] = useState("1m")
  const [seconds, setSeconds] = useState("")
  const [model, setModel] = useState("")
  const valid = !limited || (/^\d+(\.\d+)?$/.test(amount.trim()) && (period !== "custom" || (/^\d+$/.test(seconds) && Number(seconds) > 0 && Number.isSafeInteger(Number(seconds)))))
  return <RecordDialog {...props} advancedFields={props.fields.filter(field => !["name", "userId"].includes(field.name)).map(field => field.name)} advancedExtra={limited ? <Field><FieldLabel htmlFor="key-budget-model">{t("limits.appliesTo")}</FieldLabel><Input id="key-budget-model" placeholder={t("limits.allModels")} value={model} onChange={event => setModel(event.target.value)} /></Field> : null} extraAfter="name" mode="create" original={props.defaults} submitDisabled={!valid} onSubmit={body => {
    const budget: ApiKeyBudgetWrite | undefined = props.canSetBudget && limited ? { limitValue: amount.trim(), windowKey: t("limits.defaultBudget"), period, periodSeconds: period === "custom" ? Number(seconds) : null, anchorAtMs: null, modelPattern: model.trim() || null } : undefined
    props.onSubmit({ ...body, ...(budget ? { budget } : {}) })
  }} extra={<div className="flex flex-col gap-4" data-field-span="full">
    <Separator />
    {props.canSetBudget ? <>
      <Field orientation="horizontal"><FieldContent><FieldLabel htmlFor="key-budget-enabled">{t("limits.budget")}</FieldLabel><FieldDescription id="key-budget-enabled-description-0">{t(limited ? "keyBudget.activeHelp" : "keyBudget.inherit")}</FieldDescription></FieldContent><Switch aria-describedby="key-budget-enabled-description-0" aria-label={t("keyBudget.enable")} id="key-budget-enabled" checked={limited} onCheckedChange={setLimited} disabled={props.pending} /></Field>
      {limited ? <FieldGroup className="sm:grid-cols-1">
        <Field><FieldLabel htmlFor="key-budget-amount">{t("limits.costLimit")}</FieldLabel><Input id="key-budget-amount" inputMode="decimal" value={amount} onChange={event => setAmount(event.target.value)} /></Field>
        <Field><FieldLabel htmlFor="key-budget-period">{t("fields.period")}</FieldLabel><Select value={period} onValueChange={setPeriod}><SelectTrigger id="key-budget-period"><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{["5h", "1d", "7d", "1m", "total", "custom"].map(value => <SelectItem key={value} value={value}>{t(`userQuota.periods.${value}`)}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
        {period === "custom" ? <Field><FieldLabel htmlFor="key-budget-seconds">{t("fields.periodSeconds")}</FieldLabel><Input id="key-budget-seconds" type="number" min={1} step={1} value={seconds} onChange={event => setSeconds(event.target.value)} /></Field> : null}
      </FieldGroup> : null}
    </> : <p className="text-sm text-muted-foreground">{t("keyBudget.adminOnly")}</p>}
  </div>} />
}

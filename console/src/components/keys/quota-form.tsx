import { useId, useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import type { QuotaWriteRequest } from "@/generated/QuotaWriteRequest"
import { Button } from "@/components/ui/button"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"

type QuotaFormProps = {
  pending: boolean
  onSubmit: (value: QuotaWriteRequest) => Promise<void>
  fixedSubject: { kind: string; id: number }
}

const optional = (value: string) => value || null

export function QuotaForm(props: QuotaFormProps) {
  const { t } = useTranslation()
  const id = useId()
  const [total, setTotal] = useState("")
  const [daily, setDaily] = useState("")
  const [weekly, setWeekly] = useState("")
  const [monthly, setMonthly] = useState("")
  const [fiveHour, setFiveHour] = useState("")
  const [sevenDay, setSevenDay] = useState("")

  const values = [
    ["total", t("access.quotas.total"), total, setTotal, true],
    ["daily", t("access.quotas.daily"), daily, setDaily, false],
    ["weekly", t("access.quotas.weekly"), weekly, setWeekly, false],
    ["monthly", t("access.quotas.monthly"), monthly, setMonthly, false],
    ["five-hour", t("access.quotas.fiveHour"), fiveHour, setFiveHour, false],
    ["seven-day", t("access.quotas.sevenDay"), sevenDay, setSevenDay, false],
  ] as const

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    try {
      await props.onSubmit({
        subject_kind: props.fixedSubject.kind,
        subject_id: props.fixedSubject.id,
        quota_total: total,
        quota_daily: optional(daily),
        quota_weekly: optional(weekly),
        quota_monthly: optional(monthly),
        quota_5h: optional(fiveHour),
        quota_7d: optional(sevenDay),
        enabled: true,
      })
    } catch {
      return
    }
  }

  return (
    <form className="flex flex-col gap-5" onSubmit={(event) => void submit(event)}>
      <FieldGroup className="grid sm:grid-cols-2 lg:grid-cols-3">
        {values.map(([suffix, label, value, setValue, required]) => (
          <Field key={suffix}>
            <FieldLabel htmlFor={`${id}-${suffix}`}>{label}</FieldLabel>
            <Input id={`${id}-${suffix}`} type="number" inputMode="decimal" min="0" step="any" value={value} required={required} onChange={(event) => setValue(event.target.value)} />
          </Field>
        ))}
      </FieldGroup>
      <Button className="self-start" disabled={props.pending || !total}>{t("access.quotas.add")}</Button>
    </form>
  )
}

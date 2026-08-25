import { useId, useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import type { QuotaWriteRequest } from "@/generated/QuotaWriteRequest"
import { SubjectSelect, type SubjectSelectProps } from "@/components/keys/subject-select"
import { Button } from "@/components/ui/button"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"

type QuotaFormProps = Pick<SubjectSelectProps, "organizations" | "teams" | "users" | "keys"> & {
  pending: boolean
  onSubmit: (value: QuotaWriteRequest) => Promise<void>
}

const optional = (value: string) => value || null

export function QuotaForm(props: QuotaFormProps) {
  const { t } = useTranslation()
  const id = useId()
  const [kind, setKind] = useState("user_key")
  const [subjectId, setSubjectId] = useState("")
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
        subject_kind: kind,
        subject_id: Number(subjectId),
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
      <SubjectSelect {...props} kind={kind} subjectId={subjectId} onChange={(nextKind, nextId) => { setKind(nextKind); setSubjectId(nextId) }} />
      <FieldGroup className="grid sm:grid-cols-2 lg:grid-cols-3">
        {values.map(([suffix, label, value, setValue, required]) => (
          <Field key={suffix}>
            <FieldLabel htmlFor={`${id}-${suffix}`}>{label}</FieldLabel>
            <Input id={`${id}-${suffix}`} type="number" inputMode="decimal" min="0" step="any" value={value} required={required} onChange={(event) => setValue(event.target.value)} />
          </Field>
        ))}
      </FieldGroup>
      <Button className="self-start" disabled={props.pending || !subjectId || !total}>{t("access.quotas.add")}</Button>
    </form>
  )
}

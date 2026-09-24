import { useTranslation } from "react-i18next"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Button } from "@/components/ui/button"
import { useId, useState } from "react"

export type HistoryFilter = { fromMs?: number; toMs?: number; userId?: string; apiKeyId?: string; model?: string; operation?: string; requestId?: string; providerId?: string; credentialId?: string; status?: number }
const COMMON = ["userId", "apiKeyId", "model", "operation", "requestId"] as const
const UPSTREAM = ["providerId", "credentialId"] as const
export function HistoryFilters({ onApply, logs = false, summary = false }: { onApply: (filter: HistoryFilter) => void; logs?: boolean; summary?: boolean }) {
  const { t } = useTranslation()
  const id = useId()
  const [draft, setDraft] = useState<Record<string, string>>({})
  const fields = logs ? [...COMMON, ...UPSTREAM] : summary ? COMMON.filter(name => name !== "requestId") : COMMON
  function apply(event: React.FormEvent) {
    event.preventDefault()
    const result: HistoryFilter = {}
    for (const field of fields) if (draft[field]?.trim()) result[field] = draft[field].trim()
    if (draft.fromMs) result.fromMs = new Date(draft.fromMs).getTime()
    if (draft.toMs) result.toMs = new Date(draft.toMs).getTime()
    if (logs && draft.status) result.status = Number(draft.status)
    onApply(result)
  }
  return <form onSubmit={apply} className="flex flex-col gap-3">
    <FieldGroup className="grid grid-cols-2 gap-3 lg:grid-cols-4">
      {["fromMs", "toMs", ...fields, ...(logs ? ["status"] : [])].map(name => <Field key={name}>
        <FieldLabel htmlFor={`${id}-${name}`}>{t(`observation.${name}`)}</FieldLabel>
        <Input id={`${id}-${name}`} type={name.endsWith("Ms") ? "datetime-local" : name === "status" ? "number" : "text"} min={name === "status" ? 100 : name === "toMs" ? draft.fromMs : undefined} max={name === "status" ? 599 : name === "fromMs" ? draft.toMs : undefined} value={draft[name] ?? ""} onChange={event => setDraft({ ...draft, [name]: event.target.value })} />
      </Field>)}
    </FieldGroup>
    <div className="flex gap-2"><Button type="submit" size="sm">{t("observation.apply")}</Button><Button type="button" size="sm" variant="outline" onClick={() => { setDraft({}); onApply({}) }}>{t("observation.reset")}</Button></div>
  </form>
}

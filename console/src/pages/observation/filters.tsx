import { users, apiKeys } from "@/api/admin"
import { credentials, providers, providerModels } from "@/api/configuration"
import { optionSource } from "@/api/options"
import type { OptionSource } from "@/components/searchable-select"
import { useConsoleContext } from "@/capability/session"
import { SearchableSelect } from "@/components/searchable-select"
import { operationChoices } from "@/pages/providers/operation-options"
import { useTranslation } from "react-i18next"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Button } from "@/components/ui/button"
import { useId, useState } from "react"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { RESPONSE_REASONS } from "./reasons"

export type HistoryFilter = { fromMs?: number; toMs?: number; userId?: string; apiKeyId?: string; model?: string; operation?: string; requestId?: string; providerId?: string; credentialId?: string; status?: number; reason?: string }
const COMMON = ["userId", "apiKeyId", "model", "operation", "requestId"] as const
const STATUS_OPTIONS = [...new Set([200, 400, 401, 403, 404, 408, 429, 500, 502, 503, 504, ...Array.from({ length: 500 }, (_, index) => index + 100)])].map(value => ({ value: String(value), label: String(value) }))
const UPSTREAM = ["providerId", "credentialId"] as const
export function HistoryFilters({ onApply, logs = false, summary = false, reasons = false }: { onApply: (filter: HistoryFilter) => void; logs?: boolean; summary?: boolean; reasons?: boolean }) {
  const { t } = useTranslation()
  const id = useId()
  const context = useConsoleContext()
  const [draft, setDraft] = useState<Record<string, string>>({})
  const sources: Record<string, OptionSource | undefined> = {
    userId: context.has("identity.users") ? optionSource(users, row => ({ value: row.id, label: row.name })) : undefined,
    apiKeyId: context.has("identity.api-keys") ? optionSource(apiKeys, row => ({ value: row.id, label: `${row.name} · ${row.prefix}` }), { userId: draft.userId || undefined }) : undefined,
    providerId: context.has("configuration.providers") ? optionSource(providers, row => ({ value: row.id, label: row.displayName ?? row.name })) : undefined,
    credentialId: context.has("configuration.credentials") ? optionSource(credentials, row => ({ value: row.id, label: row.label ?? row.id }), { providerId: draft.providerId || undefined }) : undefined,
    model: context.has("configuration.provider-models") ? optionSource(providerModels, row => ({ value: row.upstreamName, label: row.upstreamName })) : undefined,
  }
  const options: Record<string, Array<{ value: string; label: string }>> = {
    operation: operationChoices.map(row => ({ value: row.value, label: t(`operation.${row.value}`, { defaultValue: row.value }) })),
    status: STATUS_OPTIONS,
  }
  const fields = logs ? [...COMMON, ...UPSTREAM] : summary ? COMMON.filter(name => name !== "requestId") : COMMON
  function apply(event: React.FormEvent) {
    event.preventDefault()
    const result: HistoryFilter = {}
    for (const field of fields) if (draft[field]?.trim()) result[field] = draft[field].trim()
    if (draft.fromMs) result.fromMs = new Date(draft.fromMs).getTime()
    if (draft.toMs) result.toMs = new Date(draft.toMs).getTime()
    if (reasons && draft.reason) result.reason = draft.reason
    if (logs && draft.status) result.status = Number(draft.status)
    onApply(result)
  }
  return <form onSubmit={apply} className="flex flex-col gap-3">
    <FieldGroup className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
      {["fromMs", "toMs", ...fields, ...(logs ? ["status"] : [])].map(name => <Field key={name}>
        <FieldLabel htmlFor={`${id}-${name}`}>{t(`observation.${name}`)}</FieldLabel>
        {options[name] || name in sources ? <SearchableSelect id={`${id}-${name}`} label={t(`observation.${name}`)} value={draft[name] ?? ""} options={options[name]} source={sources[name]} allowCustom={name !== "status"} emptyLabel={t("form.all")} onChange={value => setDraft(previous => ({ ...previous, [name]: value, ...(name === "userId" ? { apiKeyId: "" } : name === "providerId" ? { credentialId: "" } : {}) }))} /> : <Input id={`${id}-${name}`} type={name.endsWith("Ms") ? "datetime-local" : "text"} min={name === "toMs" ? draft.fromMs : undefined} max={name === "fromMs" ? draft.toMs : undefined} value={draft[name] ?? ""} onChange={event => setDraft({ ...draft, [name]: event.target.value })} />}
      </Field>)}
      {reasons ? <Field>
        <FieldLabel htmlFor={`${id}-reason`}>{t("observation.reason")}</FieldLabel>
        <Select value={draft.reason || "all"} onValueChange={value => setDraft({ ...draft, reason: value === "all" ? "" : value })}>
          <SelectTrigger id={`${id}-reason`} className="w-full"><SelectValue /></SelectTrigger>
          <SelectContent><SelectGroup>
            <SelectItem value="all">{t("observation.allReasons")}</SelectItem>
            {RESPONSE_REASONS.map(reason => <SelectItem key={reason} value={reason}>{t(`observation.reasons.${reason}`)}</SelectItem>)}
          </SelectGroup></SelectContent>
        </Select>
      </Field> : null}
    </FieldGroup>
    <div className="flex gap-2"><Button type="submit" size="sm">{t("observation.apply")}</Button><Button type="button" size="sm" variant="outline" onClick={() => { setDraft({}); onApply({}) }}>{t("observation.reset")}</Button></div>
  </form>
}

import type { Dispatch, SetStateAction } from "react"
import { useTranslation } from "react-i18next"
import type { LogQueryDto } from "@/generated/LogQueryDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { UserDto } from "@/generated/UserDto"
import type { UserKeyDto } from "@/generated/UserKeyDto"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Field, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"

type Props = {
  draft: LogQueryDto
  onDraft: Dispatch<SetStateAction<LogQueryDto>>
  onSearch: () => void
  providers: Array<ProviderDto>
  users: Array<UserDto>
  keys: Array<UserKeyDto>
}

const inputClass = "h-8 rounded-lg border border-input bg-transparent px-2.5 text-sm"
const datetime = (value: number) => {
  const instant = new Date(value * 1000)
  return new Date(instant.getTime() - instant.getTimezoneOffset() * 60_000).toISOString().slice(0, 16)
}
const epoch = (value: string) => Math.floor(new Date(value).getTime() / 1000)

export function LogFilters({ draft, onDraft, onSearch, providers, users, keys }: Props) {
  const { t } = useTranslation()
  const update = <K extends keyof LogQueryDto>(key: K, value: LogQueryDto[K]) => onDraft((current) => ({ ...current, [key]: value }))
  return (
    <Card size="sm">
      <CardHeader><CardTitle>{t("logs.filters.title")}</CardTitle></CardHeader>
      <CardContent>
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          <Field><FieldLabel htmlFor="logs-start">{t("logs.filters.start")}</FieldLabel><Input id="logs-start" type="datetime-local" value={datetime(draft.start)} onChange={(event) => update("start", epoch(event.target.value))} /></Field>
          <Field><FieldLabel htmlFor="logs-end">{t("logs.filters.end")}</FieldLabel><Input id="logs-end" type="datetime-local" value={datetime(draft.end)} onChange={(event) => update("end", epoch(event.target.value))} /></Field>
          <Field><FieldLabel htmlFor="logs-user">{t("logs.filters.user")}</FieldLabel><select id="logs-user" className={inputClass} value={draft.user_id ?? ""} onChange={(event) => update("user_id", event.target.value ? Number(event.target.value) : null)}><option value="">{t("logs.filters.all")}</option>{users.map((user) => <option key={user.id} value={user.id}>{user.name}</option>)}</select></Field>
          <Field><FieldLabel htmlFor="logs-key">{t("logs.filters.key")}</FieldLabel><select id="logs-key" className={inputClass} value={draft.user_key_id ?? ""} onChange={(event) => update("user_key_id", event.target.value ? Number(event.target.value) : null)}><option value="">{t("logs.filters.all")}</option>{keys.map((key) => <option key={key.id} value={key.id}>{key.label ?? key.prefix ?? `#${key.id}`}</option>)}</select></Field>
          <Field><FieldLabel htmlFor="logs-provider">{t("logs.filters.provider")}</FieldLabel><select id="logs-provider" className={inputClass} value={draft.provider_id ?? ""} onChange={(event) => update("provider_id", event.target.value ? Number(event.target.value) : null)}><option value="">{t("logs.filters.all")}</option>{providers.map((provider) => <option key={provider.id} value={provider.id}>{provider.name}</option>)}</select></Field>
          <Field><FieldLabel htmlFor="logs-status">{t("logs.filters.status")}</FieldLabel><Input id="logs-status" type="number" min={100} max={599} value={draft.status ?? ""} placeholder={t("logs.filters.statusPlaceholder")} onChange={(event) => update("status", event.target.value ? Number(event.target.value) : null)} /></Field>
          <Field className="sm:col-span-2"><FieldLabel htmlFor="logs-request-id">{t("logs.filters.requestId")}</FieldLabel><Input id="logs-request-id" className="machine-text" value={draft.request_id ?? ""} onChange={(event) => update("request_id", event.target.value || null)} /></Field>
        </div>
        <div className="mt-4 flex justify-end"><Button onClick={onSearch} disabled={!Number.isFinite(draft.start) || !Number.isFinite(draft.end) || draft.start >= draft.end}>{t("logs.filters.apply")}</Button></div>
      </CardContent>
    </Card>
  )
}

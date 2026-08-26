import type { Dispatch, SetStateAction } from "react"
import { useTranslation } from "react-i18next"
import type { LogQueryDto } from "@/generated/LogQueryDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { UserDto } from "@/generated/UserDto"
import type { UserKeyDto } from "@/generated/UserKeyDto"
import { DateRangeFilterBar } from "@/components/date-range-filter-bar"
import { SearchableSelect } from "@/components/searchable-select"
import { Field, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"

type Props = {
  draft: LogQueryDto
  onDraft: Dispatch<SetStateAction<LogQueryDto>>
  onSearch: () => void
  onReset: () => void
  providers: Array<ProviderDto>
  users: Array<UserDto>
  keys: Array<UserKeyDto>
}

const inputClass = "h-8 rounded-lg border border-input bg-transparent px-2.5 text-sm"
export function LogFilters({ draft, onDraft, onSearch, onReset, providers, users, keys }: Props) {
  const { t } = useTranslation()
  const update = <K extends keyof LogQueryDto>(key: K, value: LogQueryDto[K]) => onDraft((current) => ({ ...current, [key]: value }))
  return (
    <DateRangeFilterBar
      range={{ start: draft.start, end: draft.end }}
      onRange={({ start, end }) => onDraft((current) => ({ ...current, start, end }))}
      onApply={onSearch}
      onReset={onReset}
    >
          <Field><FieldLabel htmlFor="logs-user">{t("logs.filters.user")}</FieldLabel><select id="logs-user" className={inputClass} value={draft.user_id ?? ""} onChange={(event) => update("user_id", event.target.value ? Number(event.target.value) : null)}><option value="">{t("logs.filters.all")}</option>{users.map((user) => <option key={user.id} value={user.id}>{user.name}</option>)}</select></Field>
          <Field><FieldLabel htmlFor="logs-key">{t("logs.filters.key")}</FieldLabel><select id="logs-key" className={inputClass} value={draft.user_key_id ?? ""} onChange={(event) => update("user_key_id", event.target.value ? Number(event.target.value) : null)}><option value="">{t("logs.filters.all")}</option>{keys.map((key) => <option key={key.id} value={key.id}>{key.label ?? key.prefix ?? `#${key.id}`}</option>)}</select></Field>
          <Field><FieldLabel htmlFor="logs-provider">{t("logs.filters.provider")}</FieldLabel><SearchableSelect id="logs-provider" value={draft.provider_id == null ? "all" : String(draft.provider_id)} options={[{ value: "all", label: t("logs.filters.all") }, ...providers.map((provider) => ({ value: String(provider.id), label: provider.name }))]} placeholder={t("logs.filters.all")} searchPlaceholder={t("common.search")} emptyLabel={t("common.none")} ariaLabel={t("logs.filters.provider")} onChange={(value) => update("provider_id", value === "all" ? null : Number(value))} /></Field>
          <Field><FieldLabel htmlFor="logs-status">{t("logs.filters.status")}</FieldLabel><Input id="logs-status" type="number" min={100} max={599} value={draft.status ?? ""} placeholder={t("logs.filters.statusPlaceholder")} onChange={(event) => update("status", event.target.value ? Number(event.target.value) : null)} /></Field>
          <Field className="sm:col-span-2"><FieldLabel htmlFor="logs-request-id">{t("logs.filters.requestId")}</FieldLabel><Input id="logs-request-id" className="machine-text" value={draft.request_id ?? ""} onChange={(event) => update("request_id", event.target.value || null)} /></Field>
    </DateRangeFilterBar>
  )
}

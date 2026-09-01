import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import type { CredentialDto } from "@/generated/CredentialDto"
import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import type { UsageStatisticsDto } from "@/generated/UsageStatisticsDto"
import type { UsageQueryDto } from "@/generated/UsageQueryDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { UserDto } from "@/generated/UserDto"
import type { UserKeyDto } from "@/generated/UserKeyDto"
import { DateRangeFilterBar } from "@/components/date-range-filter-bar"
import { SearchableSelect } from "@/components/searchable-select"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Field, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { UsageTable } from "@/components/usage/usage-table"
import { WindowList } from "@/components/usage/window-list"

type Props = {
  draft: UsageQueryDto
  onDraft: (value: UsageQueryDto) => void
  onApply: () => void
  onReset: () => void
  rows: Array<UsageStatisticsDto>
  cycles: Array<CredentialQuotaCycleDto>
  credentials: Array<CredentialDto>
  providers: Array<ProviderDto>
  users: Array<UserDto>
  keys: Array<UserKeyDto>
}

export function UsageExplorer({ draft, onDraft, onApply, onReset, rows, cycles, credentials, providers, users, keys }: Props) {
  const { t } = useTranslation()
  const credentialOptions = useMemo(() => {
    const providerNames = new Map(providers.map((provider) => [provider.id, provider.name]))
    return credentials.map((credential) => ({
      value: String(credential.id),
      label: `${providerNames.get(credential.provider_id) ?? `#${credential.provider_id}`} · ${credential.label ?? `#${credential.id}`}`,
    }))
  }, [credentials, providers])
  const update = <K extends keyof UsageQueryDto>(key: K, value: UsageQueryDto[K]) => onDraft({ ...draft, [key]: value })
  return (
    <div className="flex flex-col gap-5">
      <DateRangeFilterBar
        range={{ start: draft.from, end: draft.to }}
        onRange={({ start, end }) => onDraft({ ...draft, from: start, to: end })}
        onApply={onApply}
        onReset={onReset}
      >
        <Field><FieldLabel htmlFor="usage-provider">{t("usage.filters.provider")}</FieldLabel><SearchableSelect id="usage-provider" value={draft.provider_id == null ? "all" : String(draft.provider_id)} options={[{ value: "all", label: t("usage.filters.all") }, ...providers.map((provider) => ({ value: String(provider.id), label: provider.name }))]} placeholder={t("usage.filters.all")} searchPlaceholder={t("common.search")} emptyLabel={t("common.none")} ariaLabel={t("usage.filters.provider")} onChange={(value) => update("provider_id", value === "all" ? null : Number(value))} /></Field>
        <Field><FieldLabel htmlFor="usage-credential">{t("usage.filters.credential")}</FieldLabel><SearchableSelect id="usage-credential" value={draft.credential_id == null ? "all" : String(draft.credential_id)} options={[{ value: "all", label: t("usage.filters.all") }, ...credentialOptions]} placeholder={t("usage.filters.all")} searchPlaceholder={t("common.search")} emptyLabel={t("common.none")} ariaLabel={t("usage.filters.credential")} onChange={(value) => update("credential_id", value === "all" ? null : Number(value))} /></Field>
        <Field><FieldLabel htmlFor="usage-user">{t("usage.filters.user")}</FieldLabel><SearchableSelect id="usage-user" value={draft.user_id == null ? "all" : String(draft.user_id)} options={[{ value: "all", label: t("usage.filters.all") }, ...users.map((user) => ({ value: String(user.id), label: user.name }))]} placeholder={t("usage.filters.all")} searchPlaceholder={t("common.search")} emptyLabel={t("common.none")} ariaLabel={t("usage.filters.user")} onChange={(value) => update("user_id", value === "all" ? null : Number(value))} /></Field>
        <Field><FieldLabel htmlFor="usage-key">{t("usage.filters.key")}</FieldLabel><SearchableSelect id="usage-key" value={draft.user_key_id == null ? "all" : String(draft.user_key_id)} options={[{ value: "all", label: t("usage.filters.all") }, ...keys.map((key) => ({ value: String(key.id), label: key.label ?? key.prefix ?? String(key.id) }))]} placeholder={t("usage.filters.all")} searchPlaceholder={t("common.search")} emptyLabel={t("common.none")} ariaLabel={t("usage.filters.key")} onChange={(value) => update("user_key_id", value === "all" ? null : Number(value))} /></Field>
        <Field><FieldLabel htmlFor="usage-model">{t("usage.filters.model")}</FieldLabel><Input id="usage-model" className="machine-text" value={draft.model ?? ""} onChange={(event) => update("model", event.target.value || null)} /></Field>
      </DateRangeFilterBar>
      <Tabs defaultValue="cost">
        <TabsList><TabsTrigger value="cost">{t("usage.cost.title")}</TabsTrigger><TabsTrigger value="windows">{t("usage.windows")}</TabsTrigger></TabsList>
        <TabsContent value="cost" className="pt-4"><UsageTable rows={rows} providers={providers} users={users} keys={keys} /></TabsContent>
        <TabsContent value="windows" className="pt-5"><WindowList cycles={cycles} /></TabsContent>
      </Tabs>
    </div>
  )
}

import { useMemo, type ReactNode } from "react"
import { useTranslation } from "react-i18next"
import type { CredentialDto } from "@/generated/CredentialDto"
import type { UsageRecordPageDto } from "@/generated/UsageRecordPageDto"
import type { UsageSummaryDto } from "@/generated/UsageSummaryDto"
import type { PageSize } from "@/components/data-table-pagination"
import { formatCost, formatCount } from "@/lib/format"
import type { UsageRecordQueryDto } from "@/generated/UsageRecordQueryDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { UserDto } from "@/generated/UserDto"
import type { UserKeyDto } from "@/generated/UserKeyDto"
import { DateRangeFilterBar } from "@/components/date-range-filter-bar"
import { SearchableSelect } from "@/components/searchable-select"
import { Field, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { UsageTable } from "@/components/usage/usage-table"

type Props = {
  children?: ReactNode
  draft: UsageRecordQueryDto
  onDraft: (value: UsageRecordQueryDto) => void
  onApply: () => void
  onReset: () => void
  page: UsageRecordPageDto
  summary: UsageSummaryDto | null
  summaryError: boolean
  pending: boolean
  onPage: (page: number) => void
  onPageSize: (size: PageSize) => void
  credentials: Array<CredentialDto>
  providers: Array<ProviderDto>
  users: Array<UserDto>
  keys: Array<UserKeyDto>
}

export function UsageExplorer({ children, draft, onDraft, onApply, onReset, page, summary, summaryError, pending, onPage, onPageSize, credentials, providers, users, keys }: Props) {
  const { t, i18n } = useTranslation()
  const credentialOptions = useMemo(() => {
    const providerNames = new Map(providers.map((provider) => [provider.id, provider.name]))
    return credentials.map((credential) => ({
      value: String(credential.id),
      label: `${providerNames.get(credential.provider_id) ?? `#${credential.provider_id}`} · ${credential.label ?? `#${credential.id}`}`,
    }))
  }, [credentials, providers])
  const update = <K extends keyof UsageRecordQueryDto>(key: K, value: UsageRecordQueryDto[K]) => onDraft({ ...draft, [key]: value })
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
        {(["request_id", "operation"] as const).map((key) => <Field key={key}><FieldLabel htmlFor={`usage-${key}`}>{t(key === "request_id" ? "usage.record.requestId" : "usage.record.operation")}</FieldLabel><Input id={`usage-${key}`} value={draft[key] ?? ""} onChange={(event) => update(key, event.target.value || null)} /></Field>)}
        {(["usage_source", "ended"] as const).map((key) => <Field key={key}><FieldLabel htmlFor={`usage-${key}`}>{t(key === "ended" ? "usage.record.ended" : "usage.record.source")}</FieldLabel><SearchableSelect id={`usage-${key}`} value={draft[key] ?? "all"} options={[{ value: "all", label: t("usage.filters.all") }, ...(key === "ended" ? ["complete", "interrupted"] : ["upstream", "estimated"]).map((value) => ({ value, label: t(`usage.record.${value}`) }))]} placeholder={t("usage.filters.all")} searchPlaceholder={t("common.search")} emptyLabel={t("common.none")} ariaLabel={t(key === "ended" ? "usage.record.ended" : "usage.record.source")} onChange={(value) => update(key, value === "all" ? null : value)} /></Field>)}
      </DateRangeFilterBar>
      <dl className="grid gap-4 rounded-lg border bg-card p-4 sm:grid-cols-3" aria-label={t("usage.record.summary")}>
        {[[t("usage.requests"), summary ? formatCount(summary.requests, i18n.language) : "—"], [t("usage.record.tokens"), summary ? formatCount(Number(summary.total_tokens), i18n.language) : "—"], [t("usage.cost.label"), summary ? formatCost(summary.cost, i18n.language) : "—"]].map(([label, value]) => <div key={label}><dt className="text-xs text-muted-foreground">{label}</dt><dd className="mt-1 text-xl font-semibold tabular-nums">{value}</dd></div>)}
      </dl>
      {summaryError ? <p role="alert" className="text-sm text-destructive">{t("common.loadError")}</p> : null}
      {children}
      <UsageTable page={page} providers={providers} credentials={credentials} users={users} keys={keys} pending={pending} onPage={onPage} onPageSize={onPageSize} />
    </div>
  )
}

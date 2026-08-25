import { useMemo } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { removeIdentityRule, savePermission, saveQuota, saveRateLimit } from "@/api/identity"
import type { OrganizationDto } from "@/generated/OrganizationDto"
import type { PermissionDto } from "@/generated/PermissionDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { QuotaDto } from "@/generated/QuotaDto"
import type { RateLimitDto } from "@/generated/RateLimitDto"
import type { TeamDto } from "@/generated/TeamDto"
import type { UserDto } from "@/generated/UserDto"
import type { UserKeyDto } from "@/generated/UserKeyDto"
import { PermissionForm } from "@/components/keys/permission-form"
import { QuotaForm } from "@/components/keys/quota-form"
import { RateForm } from "@/components/keys/rate-form"
import { RuleTable } from "@/components/keys/rule-table"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { formatCost } from "@/lib/format"

type AccessManagerProps = {
  organizations: Array<OrganizationDto>
  teams: Array<TeamDto>
  users: Array<UserDto>
  keys: Array<UserKeyDto>
  providers: Array<ProviderDto>
  groups: Array<string>
  permissions: Array<PermissionDto>
  rateLimits: Array<RateLimitDto>
  quotas: Array<QuotaDto>
}

export function AccessManager(props: AccessManagerProps) {
  const { t, i18n } = useTranslation()
  const queryClient = useQueryClient()
  const subjectNames = useMemo(() => new Map<string, string>([
    ...props.organizations.map((value) => [`organization:${value.id}`, value.name] as const),
    ...props.teams.map((value) => [`team:${value.id}`, value.name] as const),
    ...props.users.map((value) => [`user:${value.id}`, value.name] as const),
    ...props.keys.map((value) => [`user_key:${value.id}`, value.label ?? value.prefix ?? String(value.id)] as const),
  ]), [props.keys, props.organizations, props.teams, props.users])
  const providerNames = useMemo(() => new Map(props.providers.map((value) => [value.id, value.name])), [props.providers])
  const subject = (kind: string, id: number) => subjectNames.get(`${kind}:${id}`) ?? `${kind}:${id}`

  const permissionMutation = useMutation({
    mutationFn: ({ value, id }: { value: Parameters<typeof savePermission>[0]; id?: number }) => savePermission(value, id),
    onSuccess: async () => { toast.success(t("access.permissions.saved")); await queryClient.invalidateQueries({ queryKey: ["permissions"] }) },
    onError: () => toast.error(t("access.permissions.saveError")),
  })
  const rateMutation = useMutation({
    mutationFn: ({ value, id }: { value: Parameters<typeof saveRateLimit>[0]; id?: number }) => saveRateLimit(value, id),
    onSuccess: async () => { toast.success(t("access.rateLimits.saved")); await queryClient.invalidateQueries({ queryKey: ["rate-limits"] }) },
    onError: () => toast.error(t("access.rateLimits.saveError")),
  })
  const quotaMutation = useMutation({
    mutationFn: ({ value, id }: { value: Parameters<typeof saveQuota>[0]; id?: number }) => saveQuota(value, id),
    onSuccess: async () => { toast.success(t("access.quotas.saved")); await queryClient.invalidateQueries({ queryKey: ["quotas"] }) },
    onError: () => toast.error(t("access.quotas.saveError")),
  })
  const removeMutation = useMutation({
    mutationFn: ({ kind, id }: { kind: Parameters<typeof removeIdentityRule>[0]; id: number }) => removeIdentityRule(kind, id),
    onSuccess: async (_, value) => { await queryClient.invalidateQueries({ queryKey: [value.kind] }) },
    onError: (_, value) => toast.error(t(value.kind === "permissions" ? "access.permissions.deleteError" : value.kind === "rate-limits" ? "access.rateLimits.deleteError" : "access.quotas.deleteError")),
  })
  const shared = { organizations: props.organizations, teams: props.teams, users: props.users, keys: props.keys }
  const removing = (kind: Parameters<typeof removeIdentityRule>[0]) => removeMutation.isPending ? removeMutation.variables?.kind === kind ? removeMutation.variables.id : -1 : null
  const permissionRows = props.permissions.map((value) => ({
    id: value.id,
    subject: subject(value.subject_kind, value.subject_id),
    detail: [value.provider_id == null ? t("access.permissions.allProviders") : providerNames.get(value.provider_id) ?? value.provider_id, value.operation_group ?? t("access.permissions.allOperations"), t(value.allowed ? "access.permissions.allow" : "access.permissions.deny")].join(" · "),
  }))
  const rateRows = props.rateLimits.map((value) => ({ id: value.id, subject: subject(value.subject_kind, value.subject_id), detail: t("access.rateLimits.summary", { requests: value.requests, seconds: value.window_seconds }) }))
  const quotaRows = props.quotas.map((value) => ({
    id: value.id,
    subject: subject(value.subject_kind, value.subject_id),
    detail: [...[["total", value.quota_total], ["daily", value.quota_daily], ["weekly", value.quota_weekly], ["monthly", value.quota_monthly], ["fiveHour", value.quota_5h], ["sevenDay", value.quota_7d]].flatMap(([label, amount]) => amount == null ? [] : [`${t(`access.quotas.${label}`)}: ${formatCost(amount, i18n.language)}`]), t(value.enabled ? "common.status.enabled" : "common.status.disabled")].join(" · "),
  }))

  return (
    <Card>
      <CardHeader><CardTitle>{t("access.title")}</CardTitle><CardDescription>{t("access.subtitle")}</CardDescription></CardHeader>
      <CardContent><Tabs defaultValue="permissions">
        <TabsList className="max-w-full overflow-x-auto"><TabsTrigger value="permissions">{t("access.permissions.title")}</TabsTrigger><TabsTrigger value="rates">{t("access.rateLimits.title")}</TabsTrigger><TabsTrigger value="quotas">{t("access.quotas.title")}</TabsTrigger></TabsList>
        <TabsContent value="permissions" className="flex flex-col gap-6 pt-5"><PermissionForm {...shared} providers={props.providers} groups={props.groups} pending={permissionMutation.isPending} onSubmit={(value) => {
          const stored = props.permissions.find((item) => item.subject_kind === value.subject_kind && item.subject_id === value.subject_id && item.provider_id === value.provider_id && item.operation_group === value.operation_group)
          return permissionMutation.mutateAsync({ value, id: stored?.id }).then(() => undefined)
        }} /><RuleTable rows={permissionRows} empty={t("access.permissions.empty")} removeLabel={t("access.permissions.delete")} removingId={removing("permissions")} remove={(id) => removeMutation.mutate({ kind: "permissions", id })} /></TabsContent>
        <TabsContent value="rates" className="flex flex-col gap-6 pt-5"><RateForm {...shared} pending={rateMutation.isPending} onSubmit={(value) => {
          const stored = props.rateLimits.find((item) => item.subject_kind === value.subject_kind && item.subject_id === value.subject_id && item.window_seconds === value.window_seconds)
          return rateMutation.mutateAsync({ value, id: stored?.id }).then(() => undefined)
        }} /><RuleTable rows={rateRows} empty={t("access.rateLimits.empty")} removeLabel={t("access.rateLimits.delete")} removingId={removing("rate-limits")} remove={(id) => removeMutation.mutate({ kind: "rate-limits", id })} /></TabsContent>
        <TabsContent value="quotas" className="flex flex-col gap-6 pt-5"><QuotaForm {...shared} pending={quotaMutation.isPending} onSubmit={(value) => {
          const stored = props.quotas.find((item) => item.subject_kind === value.subject_kind && item.subject_id === value.subject_id)
          return quotaMutation.mutateAsync({ value, id: stored?.id }).then(() => undefined)
        }} /><RuleTable rows={quotaRows} empty={t("access.quotas.empty")} removeLabel={t("access.quotas.delete")} removingId={removing("quotas")} remove={(id) => removeMutation.mutate({ kind: "quotas", id })} /></TabsContent>
      </Tabs></CardContent>
    </Card>
  )
}

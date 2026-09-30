import { useState, type FormEvent } from "react"
import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import type { RouteMemberDto } from "@/generated/sdk"
import { discoverModels } from "@/api/models"
import { providers, providerModels } from "@/api/configuration"
import { ManagementDialog } from "@/components/management-dialog"
import { ErrorNotice } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import { SearchableSelect } from "@/components/searchable-select"
import { optionSource } from "@/api/options"
export function MemberForm({ original, onSubmit, onClose, pending, error }: { original?: RouteMemberDto; onSubmit: (body: Record<string, unknown>) => void; onClose: () => void; pending: boolean; error: unknown }) {
  const { t } = useTranslation()
  const [providerId, setProviderId] = useState(original?.providerId ?? "")
  const [model, setModel] = useState(original?.upstreamModel ?? "")
  const [tier, setTier] = useState(String(original?.tier ?? 0))
  const [weight, setWeight] = useState(String(original?.weight ?? 1))
  const [enabled, setEnabled] = useState(original?.enabled ?? true)
  const provider = useQuery({ queryKey: ["admin", "/providers", "detail", providerId], queryFn: () => providers.get(providerId), enabled: !!providerId })
  const refresh = (provider.data?.config as { auto_refresh_models?: boolean } | null)?.auto_refresh_models !== false
  const discovered = useQuery({
    queryKey: ["route-member-models", providerId, refresh],
    queryFn: () => discoverModels(providerId),
    enabled: !!provider.data && refresh,
    retry: false,
    refetchOnWindowFocus: false,
    refetchOnMount: "always",
  })
  const candidates = refresh && !discovered.isError ? discovered.data : undefined
  const submit = (event: FormEvent) => {
    event.preventDefault()
    const body: Record<string, unknown> = { providerId, upstreamModel: model.trim(), tier: Number(tier), weight: Number(weight), enabled }
    if (original) for (const key of Object.keys(body)) if (body[key] === original[key as keyof RouteMemberDto]) delete body[key]
    onSubmit(body)
  }
  return <ManagementDialog title={t(original ? "edit.route-members" : "create.route-members")} onClose={onClose} busy={pending}><form onSubmit={submit}><FieldGroup>
    <Field><FieldLabel htmlFor="member-provider">{t("fields.providerId")}</FieldLabel><SearchableSelect id="member-provider" label={t("fields.providerId")} value={providerId} onChange={id => { setProviderId(id); setModel("") }} disabled={pending} source={optionSource(providers, row => ({ value: row.id, label: row.displayName ?? row.name }))} options={provider.data ? [{ value: provider.data.id, label: provider.data.displayName ?? provider.data.name }] : []} /></Field>
    <Field><FieldLabel htmlFor="member-model">{t("fields.upstreamModel")}</FieldLabel><SearchableSelect id="member-model" label={t("fields.upstreamModel")} value={model} onChange={setModel} disabled={pending || !providerId} allowCustom options={candidates?.map(row => ({ value: row.upstreamName, label: row.upstreamName }))} source={candidates ? undefined : optionSource(providerModels, row => ({ value: row.upstreamName, label: row.upstreamName }), { providerId })} />{refresh && discovered.isFetching ? <FieldDescription>{t("modelRoutes.modelsLoading")}</FieldDescription> : null}{refresh && discovered.isError ? <FieldDescription role="status">{t("modelRoutes.modelsFallback")}</FieldDescription> : null}</Field>
    <Field><FieldLabel htmlFor="member-tier">{t("fields.tier")}</FieldLabel><Input id="member-tier" type="number" min={0} step={1} required value={tier} onChange={e => setTier(e.target.value)} disabled={pending} /></Field>
    <Field><FieldLabel htmlFor="member-weight">{t("fields.weight")}</FieldLabel><Input id="member-weight" type="number" min={1} step={1} required value={weight} onChange={e => setWeight(e.target.value)} disabled={pending} /></Field>
    <Field orientation="horizontal"><FieldLabel htmlFor="member-enabled">{t("fields.enabled")}</FieldLabel><Switch id="member-enabled" checked={enabled} onCheckedChange={setEnabled} disabled={pending} /></Field>
    {error || provider.error ? <ErrorNotice error={error ?? provider.error} /> : null}<Button type="submit" disabled={pending || !providerId || !model.trim()}>{t("actions.save")}</Button>
  </FieldGroup></form></ManagementDialog>
}

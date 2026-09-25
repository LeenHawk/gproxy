import { useState, type FormEvent } from "react"
import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import type { CredentialProviderDto } from "@/generated/app"
import type { RouteMemberDto } from "@/generated/sdk"
import { directory } from "@/api/models"
import { providerModels } from "@/api/configuration"
import { ManagementDialog } from "@/components/management-dialog"
import { ErrorNotice } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
export function MemberForm({ original, providers, onSubmit, onClose, pending, error }: { original?: RouteMemberDto; providers: CredentialProviderDto[]; onSubmit: (body: Record<string, unknown>) => void; onClose: () => void; pending: boolean; error: unknown }) {
  const { t } = useTranslation()
  const [providerId, setProviderId] = useState(original?.providerId ?? "")
  const [model, setModel] = useState(original?.upstreamModel ?? "")
  const [tier, setTier] = useState(String(original?.tier ?? 0))
  const [weight, setWeight] = useState(String(original?.weight ?? 1))
  const [enabled, setEnabled] = useState(original?.enabled ?? true)
  const models = useQuery({ queryKey: ["admin", "/provider-models", "directory", providerId], queryFn: () => directory(providerModels, { providerId }), enabled: !!providerId })
  const submit = (event: FormEvent) => {
    event.preventDefault()
    const body: Record<string, unknown> = { providerId, upstreamModel: model.trim(), tier: Number(tier), weight: Number(weight), enabled }
    if (original) for (const key of Object.keys(body)) if (body[key] === original[key as keyof RouteMemberDto]) delete body[key]
    onSubmit(body)
  }
  return <ManagementDialog title={t(original ? "edit.route-members" : "create.route-members")} onClose={onClose} busy={pending}><form onSubmit={submit}><FieldGroup>
    <Field><FieldLabel htmlFor="member-provider">{t("fields.providerId")}</FieldLabel><Select value={providerId} onValueChange={id => { setProviderId(id); setModel("") }} disabled={pending}><SelectTrigger id="member-provider"><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{providers.map(row => <SelectItem key={row.id} value={row.id}>{row.displayName ?? row.name}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
    <Field><FieldLabel htmlFor="member-model">{t("fields.upstreamModel")}</FieldLabel><Input id="member-model" list="member-models" required value={model} onChange={e => setModel(e.target.value)} disabled={pending} /><datalist id="member-models">{models.data?.map(row => <option key={row.id} value={row.upstreamName} />)}</datalist></Field>
    <Field><FieldLabel htmlFor="member-tier">{t("fields.tier")}</FieldLabel><Input id="member-tier" type="number" min={0} step={1} required value={tier} onChange={e => setTier(e.target.value)} disabled={pending} /></Field>
    <Field><FieldLabel htmlFor="member-weight">{t("fields.weight")}</FieldLabel><Input id="member-weight" type="number" min={1} step={1} required value={weight} onChange={e => setWeight(e.target.value)} disabled={pending} /></Field>
    <Field orientation="horizontal"><FieldLabel htmlFor="member-enabled">{t("fields.enabled")}</FieldLabel><Switch id="member-enabled" checked={enabled} onCheckedChange={setEnabled} disabled={pending} /></Field>
    {error || models.error ? <ErrorNotice error={error ?? models.error} /> : null}<Button type="submit" disabled={pending || !providerId || !model.trim()}>{t("actions.save")}</Button>
  </FieldGroup></form></ManagementDialog>
}

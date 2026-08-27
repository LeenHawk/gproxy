import { useMemo, useState } from "react"
import { PlusIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { saveRouteMember } from "@/api/control"
import type { CredentialDto } from "@/generated/CredentialDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { RouteDto } from "@/generated/RouteDto"
import type { RouteMemberDto } from "@/generated/RouteMemberDto"
import { DataTable, type DataTableColumn } from "@/components/data-table"
import { BatchActions } from "@/components/batch-actions"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { EnabledSwitch } from "@/components/routes/enabled-switch"
import { MemberForm } from "@/components/routes/member-form"

export function MembersPanel({
  route,
  members,
  providers,
  credentials,
  onChanged,
}: {
  route: RouteDto
  members: Array<RouteMemberDto>
  providers: Array<ProviderDto>
  credentials: Array<CredentialDto>
  onChanged: () => void
}) {
  const { t } = useTranslation()
  const [form, setForm] = useState<{ member: RouteMemberDto | null; opener: HTMLElement } | null>(null)
  const routeMembers = useMemo(
    () => members.filter((member) => member.route_id === route.id).sort((a, b) => a.tier - b.tier || b.weight - a.weight || a.id - b.id),
    [members, route.id],
  )
  const providerById = useMemo(() => new Map(providers.map((provider) => [provider.id, provider])), [providers])
  const credentialById = useMemo(
    () => new Map(credentials.map((credential) => [credential.id, credential])),
    [credentials],
  )

  function openForm(value: RouteMemberDto | null, element: HTMLElement) {
    setForm({ member: value, opener: element })
  }
  const credentialLabel = (member: RouteMemberDto) => {
    if (member.credential_id == null) return t("routes.members.anyCredential")
    return credentialById.get(member.credential_id)?.label ?? member.credential_id
  }
  const actions = (member: RouteMemberDto) => <div className="flex items-center justify-end gap-2" onClick={(event) => event.stopPropagation()}>
    <EnabledSwitch
      checked={member.enabled}
      label={`${member.upstream_model}: ${t("routes.members.enabled")}`}
      errorMessage={t("routes.members.saveError")}
      onChange={(enabled) => saveRouteMember({ ...member, enabled }, member.id)}
      onChanged={onChanged}
    />
    <Button size="sm" variant="outline" aria-label={`${t("common.actions.edit")}: ${member.upstream_model}`} onClick={(event) => openForm(member, event.currentTarget)}>{t("common.actions.edit")}</Button>
  </div>
  const columns: Array<DataTableColumn<RouteMemberDto>> = [
    { key: "provider", label: t("routes.members.provider"), header: t("routes.members.provider"), cell: (member) => providerById.get(member.provider_id)?.name ?? member.provider_id },
    { key: "credential", label: t("routes.members.credential"), header: t("routes.members.credential"), cell: (member) => <span className="font-mono text-xs">{credentialLabel(member)}</span> },
    { key: "model", label: t("routes.members.model"), header: t("routes.members.model"), cell: (member) => <span className="font-mono text-xs">{member.upstream_model}</span> },
    { key: "tier", label: t("routes.members.tier"), header: t("routes.members.tier"), cell: (member) => <span className="font-mono text-xs">{member.tier}</span> },
    { key: "weight", label: t("routes.members.weight"), header: t("routes.members.weight"), cell: (member) => <span className="font-mono text-xs">{member.weight}</span> },
    { key: "actions", label: t("common.actions.edit"), header: <span className="sr-only">{t("common.actions.edit")}</span>, cell: actions },
  ]

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("routes.members.title")}</CardTitle>
        <CardDescription>{route.name}</CardDescription>
        <CardAction>
          <Button size="sm" disabled={providers.length === 0} onClick={(event) => openForm(null, event.currentTarget)}>
            <PlusIcon data-icon="inline-start" />
            {t("routes.members.add")}
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent>
        <DataTable
          columns={columns}
          rows={routeMembers}
          rowKey={(member) => member.id}
          searchText={(member) => `${providerById.get(member.provider_id)?.name ?? member.provider_id} ${credentialLabel(member)} ${member.upstream_model}`}
          renderCard={(member) => <div className="flex flex-col gap-3"><div><p className="font-medium">{providerById.get(member.provider_id)?.name ?? member.provider_id}</p><p className="font-mono text-xs text-muted-foreground">{member.upstream_model}</p><p className="text-xs text-muted-foreground">{t("routes.members.tier")}: {member.tier} · {t("routes.members.weight")}: {member.weight}</p></div>{actions(member)}</div>}
          empty={t("routes.members.empty")}
          storageKey="route-members"
          selectable
          batchActions={(rows) => <BatchActions entity="route-members" rows={rows} queryKeys={["route-members"]} />}
        />
      </CardContent>
      {form ? (
        <MemberForm
          key={form.member?.id ?? "new"}
          route={route}
          member={form.member}
          providers={providers}
          credentials={credentials}
          opener={form.opener}
          onOpenChange={(open) => { if (!open) setForm(null) }}
          onChanged={onChanged}
        />
      ) : null}
    </Card>
  )
}

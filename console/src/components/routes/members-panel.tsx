import { useMemo, useState } from "react"
import { PlusIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { saveRouteMember } from "@/api/control"
import type { CredentialDto } from "@/generated/CredentialDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { RouteDto } from "@/generated/RouteDto"
import type { RouteMemberDto } from "@/generated/RouteMemberDto"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Empty, EmptyContent, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
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
        {routeMembers.length === 0 ? (
          <Empty>
            <EmptyHeader><EmptyTitle>{t("routes.members.empty")}</EmptyTitle></EmptyHeader>
            <EmptyContent>
              <Button disabled={providers.length === 0} onClick={(event) => openForm(null, event.currentTarget)}>{t("routes.members.add")}</Button>
            </EmptyContent>
          </Empty>
        ) : (
          <Table>
            <TableHeader><TableRow>
              <TableHead>{t("routes.members.provider")}</TableHead>
              <TableHead>{t("routes.members.credential")}</TableHead>
              <TableHead>{t("routes.members.model")}</TableHead>
              <TableHead>{t("routes.members.tier")}</TableHead>
              <TableHead>{t("routes.members.weight")}</TableHead>
              <TableHead>{t("routes.members.enabled")}</TableHead>
              <TableHead><span className="sr-only">{t("common.actions.edit")}</span></TableHead>
            </TableRow></TableHeader>
            <TableBody>{routeMembers.map((member) => {
              const credential = member.credential_id == null ? null : credentialById.get(member.credential_id)
              return (
                <TableRow key={member.id}>
                  <TableCell>{providerById.get(member.provider_id)?.name ?? member.provider_id}</TableCell>
                  <TableCell className="font-mono text-xs">
                    {member.credential_id == null
                      ? t("routes.members.anyCredential")
                      : credential?.label ?? member.credential_id}
                  </TableCell>
                  <TableCell className="font-mono text-xs">{member.upstream_model}</TableCell>
                  <TableCell className="font-mono tabular-nums">{member.tier}</TableCell>
                  <TableCell className="font-mono tabular-nums">{member.weight}</TableCell>
                  <TableCell>
                    <EnabledSwitch
                      checked={member.enabled}
                      label={`${member.upstream_model}: ${t("routes.members.enabled")}`}
                      errorMessage={t("routes.members.saveError")}
                      onChange={(enabled) => saveRouteMember({
                        route_id: member.route_id,
                        provider_id: member.provider_id,
                        credential_id: member.credential_id,
                        upstream_model: member.upstream_model,
                        tier: member.tier,
                        weight: member.weight,
                        enabled,
                      }, member.id)}
                      onChanged={onChanged}
                    />
                  </TableCell>
                  <TableCell className="text-right">
                    <Button size="sm" variant="outline" aria-label={`${t("common.actions.edit")}: ${member.upstream_model}`} onClick={(event) => openForm(member, event.currentTarget)}>{t("common.actions.edit")}</Button>
                  </TableCell>
                </TableRow>
              )
            })}</TableBody>
          </Table>
        )}
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

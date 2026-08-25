import { useRef, useState, type MouseEvent } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { PlusIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import {
  createUserKey,
  revealUserKey,
  saveOrganization,
  saveTeam,
  saveUser,
  updateUserKey,
} from "@/api/identity"
import type { OrganizationDto } from "@/generated/OrganizationDto"
import type { OrganizationWriteRequest } from "@/generated/OrganizationWriteRequest"
import type { TeamDto } from "@/generated/TeamDto"
import type { TeamWriteRequest } from "@/generated/TeamWriteRequest"
import type { UserDto } from "@/generated/UserDto"
import type { UserKeyCreateResponse } from "@/generated/UserKeyCreateResponse"
import type { UserKeyDto } from "@/generated/UserKeyDto"
import type { UserWriteRequest } from "@/generated/UserWriteRequest"
import { CreatedKeyDialog } from "@/components/keys/created-key-dialog"
import { IdentityForms } from "@/components/keys/identity-forms"
import { IdentityTable } from "@/components/keys/identity-table"
import { KeyForm } from "@/components/keys/key-form"
import { KeyTable } from "@/components/keys/key-table"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Dialog } from "@/components/ui/dialog"
import { Empty, EmptyContent, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { Separator } from "@/components/ui/separator"

type KeyManagementProps = {
  organizations: Array<OrganizationDto>
  teams: Array<TeamDto>
  users: Array<UserDto>
  keys: Array<UserKeyDto>
}

export function KeyManagement(props: KeyManagementProps) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const keyOpenerRef = useRef<HTMLButtonElement | null>(null)
  const [keyFormOpen, setKeyFormOpen] = useState(false)
  const [createdKey, setCreatedKey] = useState<UserKeyCreateResponse | null>(null)
  const organizationMutation = useMutation({
    mutationFn: ({ value, id }: { value: OrganizationWriteRequest; id?: number }) => saveOrganization(value, id),
    onSuccess: async () => { toast.success(t("common.actions.saved")); await queryClient.invalidateQueries({ queryKey: ["organizations"] }) },
    onError: () => toast.error(t("common.errors.save")),
  })
  const teamMutation = useMutation({
    mutationFn: ({ value, id }: { value: TeamWriteRequest; id?: number }) => saveTeam(value, id),
    onSuccess: async () => { toast.success(t("common.actions.saved")); await queryClient.invalidateQueries({ queryKey: ["teams"] }) },
    onError: () => toast.error(t("common.errors.save")),
  })
  const userMutation = useMutation({
    mutationFn: ({ value, id }: { value: UserWriteRequest; id?: number }) => saveUser(value, id),
    onSuccess: async (_, input) => { toast.success(t(input.id == null ? "users.created" : "users.updated")); await queryClient.invalidateQueries({ queryKey: ["users"] }) },
    onError: () => toast.error(t("users.saveError")),
  })
  const keyMutation = useMutation({
    mutationFn: createUserKey,
    onSuccess: async (value) => {
      setKeyFormOpen(false)
      setCreatedKey(value)
      await queryClient.invalidateQueries({ queryKey: ["user-keys"] })
    },
    onError: () => toast.error(t("users.keys.createError")),
  })
  const keyUpdateMutation = useMutation({
    mutationFn: (key: UserKeyDto) => updateUserKey(key.id, { label: key.label, expires_at: key.expires_at, enabled: !key.enabled }),
    onSuccess: async () => { toast.success(t("users.keys.updated")); await queryClient.invalidateQueries({ queryKey: ["user-keys"] }) },
    onError: () => toast.error(t("users.keys.updateError")),
  })
  const identityPending = organizationMutation.isPending || teamMutation.isPending || userMutation.isPending
  const openKeyForm = (event: MouseEvent<HTMLButtonElement>) => {
    keyOpenerRef.current = event.currentTarget
    setKeyFormOpen(true)
  }
  const returnKeyFocus = () => keyOpenerRef.current?.focus()

  return (
    <div className="flex flex-col gap-6">
      <Card>
        <CardHeader><CardTitle>{t("users.title")}</CardTitle><CardDescription>{t("users.subtitle")}</CardDescription></CardHeader>
        <CardContent className="flex flex-col gap-6">
          <IdentityForms
            organizations={props.organizations}
            teams={props.teams}
            pending={identityPending}
            onOrganization={(name) => organizationMutation.mutateAsync({ value: { name, enabled: true } }).then(() => undefined)}
            onTeam={(organizationId, name) => teamMutation.mutateAsync({ value: { organization_id: organizationId, name, enabled: true } }).then(() => undefined)}
            onUser={(organizationId, teamId, name) => userMutation.mutateAsync({ value: { organization_id: organizationId, team_id: teamId, name, enabled: true } }).then(() => undefined)}
          />
          <Separator />
          <IdentityTable
            {...props}
            pending={identityPending}
            onOrganizationToggle={(value) => organizationMutation.mutate({ id: value.id, value: { name: value.name, enabled: !value.enabled } })}
            onTeamToggle={(value) => teamMutation.mutate({ id: value.id, value: { organization_id: value.organization_id, name: value.name, enabled: !value.enabled } })}
            onUserToggle={(value) => userMutation.mutate({ id: value.id, value: { organization_id: value.organization_id, team_id: value.team_id, name: value.name, enabled: !value.enabled } })}
          />
        </CardContent>
      </Card>
      <Card>
        <CardHeader>
          <CardTitle>{t("users.keys.title")}</CardTitle>
          <CardAction><Button type="button" size="sm" disabled={props.users.length === 0} onClick={openKeyForm}><PlusIcon data-icon="inline-start" />{t("users.keys.create")}</Button></CardAction>
        </CardHeader>
        <CardContent>
          {props.keys.length === 0 ? (
            <Empty><EmptyHeader><EmptyTitle>{t("users.keys.empty")}</EmptyTitle></EmptyHeader><EmptyContent><Button type="button" disabled={props.users.length === 0} onClick={openKeyForm}>{t("users.keys.create")}</Button></EmptyContent></Empty>
          ) : (
            <KeyTable keys={props.keys} users={props.users} pending={keyUpdateMutation.isPending} reveal={revealUserKey} onEnabledChange={keyUpdateMutation.mutate} />
          )}
        </CardContent>
      </Card>
      <Dialog open={keyFormOpen} onOpenChange={setKeyFormOpen}>
        <KeyForm users={props.users} pending={keyMutation.isPending} returnFocus={returnKeyFocus} onSubmit={(value) => keyMutation.mutateAsync(value).then(() => undefined)} />
      </Dialog>
      <CreatedKeyDialog value={createdKey} onClose={() => setCreatedKey(null)} returnFocus={returnKeyFocus} />
    </div>
  )
}

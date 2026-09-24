import { KeyCreateDialog } from "@/components/keys/create-dialog"
import { KeySettingsDialog } from "@/components/keys/settings-dialog"
import { RecordDialog, type FormField } from "@/components/record-form"
import { providers } from "@/api/configuration"
import { directory, models } from "@/api/models"
import { useConsoleContext } from "@/capability/session"
import { ScopedBudgetObjects } from "./scoped-budgets"
import { QuotaButton } from "@/pages/quotas"
//! The identity families, each one a declaration.
//!
//! Twelve pages, twelve declarations, one [`CollectionPage`]. A family that
//! needs more than the five routes says so in `rowActions` — a key's rotate
//! and reveal, a client's retire — and nothing else here is bespoke.
//!
//! The field lists are the DTOs' write and patch shapes, in the DTO's order.
//! Where a column is immutable the field is `createOnly` (a team's
//! organization), and where it is nullable the field is
//! `nullable`, which is what makes an emptied input send the `null` that
//! `double_option` turns into "clear it".

import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import * as admin from "@/api/admin"
import { BoolCell, InstantCell, MaybeCell } from "@/components/cells"
import { ConfirmButton } from "@/components/confirm"
import { IdCell } from "@/components/data-table"
import { KeySecretCell } from "@/components/key-secret-cell"
import { SecretDialog } from "@/components/secret-dialog"
import { Button } from "@/components/ui/button"
import type {
  ApiKeyCreated, ApiKeyDto, OAuthClientDto, OrganizationDto, PermissionDto, RateLimitDto, TeamDto, UserDto,
} from "@/generated/app"
import { CollectionPage } from "@/pages/identity/collection"
import { UserQuotasDialog } from "@/pages/identity/user-quotas"
import { MembersDialog } from "@/pages/identity/members"

const ROLES = ["user", "admin"] as const
const ACTIONS = ["allow", "deny"] as const

// ----------------------------------------------------------------- users --

export function UsersPage() {
  const { t } = useTranslation()
  const [quotaUser, setQuotaUser] = useState<UserDto | null>(null)
  return (
    <>
      <CollectionPage
        id="users"
        rowActions={(row) => <Button variant="ghost" size="sm" onClick={() => setQuotaUser(row)}>{t("limits.budget")}</Button>}
        family={admin.users}
        searchable
        rowId={(row: UserDto) => row.id}
        rowLabel={(row) => row.name}
        columns={[
          { key: "name", cell: (row) => row.name },
          { key: "role", cell: (row) => <MaybeCell value={row.role} /> },
          { key: "enabled", cell: (row) => <BoolCell value={row.enabled} /> },
          { key: "hasPassword", cell: (row) => <BoolCell value={row.hasPassword} /> },
          { key: "createdAtMs", cell: (row) => <InstantCell value={row.createdAtMs} /> },
          { key: "id", cell: (row) => <IdCell value={row.id} /> },
        ]}
        fields={[
          { name: "name", kind: "text", required: true },
          { name: "password", kind: "password", createOnly: true },
          { name: "role", kind: "select", options: ROLES },
          { name: "enabled", kind: "switch" },
          { name: "oauthClientAllowlist", kind: "lines", nullable: true },
        ]}
      />
      {quotaUser ? <UserQuotasDialog key={quotaUser.id} userId={quotaUser.id} userName={quotaUser.name} onClose={() => setQuotaUser(null)} /> : null}
    </>
  )
}

// -------------------------------------------------------------- api keys --

export function ApiKeysPage() {
  const { t } = useTranslation()
  const client = useQueryClient()
  const context = useConsoleContext()
  const [editing, setEditing] = useState<{ row: ApiKeyDto; tab: "basic" | "budget" } | null>(null)
  const users = useQuery({ queryKey: ["admin", "/users", "directory"], queryFn: () => directory(admin.users) })
  const fields: FormField[] = [
    { name: "userId", kind: "select", required: true, createOnly: true, choices: (users.data ?? []).map(user => ({ value: user.id, label: user.name })) },
    { name: "name", kind: "text", required: true },
    { name: "organizationId", kind: "text", nullable: true },
    { name: "teamId", kind: "text", nullable: true },
    { name: "expiresAtMs", kind: "datetime", nullable: true },
    { name: "enabled", kind: "switch" },
    { name: "retainSecret", kind: "switch", createOnly: true },
  ]
  const [token, setToken] = useState<string | null>(null)
  const invalidate = () => client.invalidateQueries({ queryKey: ["admin", admin.apiKeys.path] })

  const rotate = useMutation({
    mutationFn: (id: string) => admin.apiKeys.rotate(id),
    onSuccess: (created) => { setToken(created.token); void invalidate() },
    onError: (error: Error) => toast.error(error.message),
  })

  return (
    <>
      <CollectionPage
        id="api-keys"
        family={admin.apiKeys}
        searchable
        create={(body) => admin.createApiKey(body as Parameters<typeof admin.createApiKey>[0])}
        onCreated={(created) => { setToken((created as ApiKeyCreated).token); void client.invalidateQueries({ queryKey: ["portal", "keys"] }); void client.invalidateQueries({ queryKey: ["admin", "/quotas"] }) }}
        rowId={(row: ApiKeyDto) => row.id}
        rowLabel={(row) => row.name}
        columns={[
          { key: "name", cell: (row) => row.name },
          { key: "prefix", header: t("keys.value"), cell: (row) => <KeySecretCell key={`${row.id}:${row.prefix}`} prefix={row.prefix} revealable={row.hasSecret && row.kind !== "oauth"} reveal={() => admin.apiKeys.reveal(row.id)} /> },
          { key: "kind", cell: (row) => <MaybeCell value={row.kind} /> },
          { key: "userId", cell: (row) => <IdCell value={row.userId} /> },
          { key: "enabled", cell: (row) => <BoolCell value={row.enabled} /> },
          { key: "expiresAtMs", cell: (row) => <InstantCell value={row.expiresAtMs} /> },
        ]}
        fields={fields}
        onEdit={row => setEditing({ row, tab: "basic" })}
        renderForm={props => <KeyCreateDialog {...props} error={props.error ?? users.error} pending={props.pending || users.isPending} fields={fields} defaults={{ userId: context.userId, enabled: true, retainSecret: true }} title={t("create.api-keys")} canSetBudget={context.has("configuration.quotas.write")} />}

        rowActions={(row) => (
          <>
            <ConfirmButton
              title={t("confirm.rotateTitle", { name: row.name })}
              confirmLabel={t("actions.rotate")}
              onConfirm={() => rotate.mutate(row.id)}
            >
              {t("actions.rotate")}
            </ConfirmButton>
          </>
        )}
      />
      {editing ? <KeySettingsDialog apiKey={editing.row} fields={fields} initialTab={editing.tab} onClose={() => setEditing(null)} /> : null}
      <SecretDialog token={token} onClose={() => setToken(null)} />
    </>
  )
}

// --------------------------------------------------------- organizations --

export function OrganizationsPage() {
  const context = useConsoleContext()
  return context.has("identity.organizations") ? <InstanceOrganizationsPage /> : <ScopedBudgetObjects kind="org" />
}

function InstanceOrganizationsPage() {
  const { t } = useTranslation()
  const [members, setMembers] = useState<OrganizationDto | null>(null)
  return (
    <>
      <CollectionPage
        id="organizations"
        family={admin.organizations}
        searchable
        rowId={(row: OrganizationDto) => row.id}
        rowLabel={(row) => row.name}
        columns={[
          { key: "name", cell: (row) => row.name },
          { key: "createdAtMs", cell: (row) => <InstantCell value={row.createdAtMs} /> },
          { key: "id", cell: (row) => <IdCell value={row.id} /> },
        ]}
        fields={[
          { name: "name", kind: "text", required: true },
          { name: "oauthClientAllowlist", kind: "lines", nullable: true },
        ]}
        rowActions={(row) => (
          <><QuotaButton ownerKind="org" ownerId={row.id} name={row.name} /><Button variant="ghost" size="sm" onClick={() => setMembers(row)}>{t("actions.members")}</Button></>
        )}
      />
      <MembersDialog
        scope="organization"
        scopeId={members?.id ?? ""}
        scopeName={members?.name ?? ""}
        open={members !== null}
        onOpenChange={(open) => { if (!open) setMembers(null) }}
      />
    </>
  )
}

// ----------------------------------------------------------------- teams --

export function TeamsPage() {
  const context = useConsoleContext()
  return context.has("identity.teams") ? <InstanceTeamsPage /> : <ScopedBudgetObjects kind="team" />
}

function InstanceTeamsPage() {
  const { t } = useTranslation()
  const [members, setMembers] = useState<TeamDto | null>(null)
  return (
    <>
      <CollectionPage
        id="teams"
        family={admin.teams}
        searchable
        rowId={(row: TeamDto) => row.id}
        rowLabel={(row) => row.name}
        columns={[
          { key: "name", cell: (row) => row.name },
          { key: "organizationId", cell: (row) => <IdCell value={row.organizationId} /> },
          { key: "createdAtMs", cell: (row) => <InstantCell value={row.createdAtMs} /> },
          { key: "id", cell: (row) => <IdCell value={row.id} /> },
        ]}
        fields={[
          // A team cannot be moved between organizations: its credentials,
          // budgets and bound keys were all scoped under the parent.
          { name: "organizationId", kind: "text", required: true, createOnly: true },
          { name: "name", kind: "text", required: true },
          { name: "oauthClientAllowlist", kind: "lines", nullable: true },
        ]}
        rowActions={(row) => (
          <><QuotaButton ownerKind="team" ownerId={row.id} name={row.name} /><Button variant="ghost" size="sm" onClick={() => setMembers(row)}>{t("actions.members")}</Button></>
        )}
      />
      <MembersDialog
        scope="team"
        scopeId={members?.id ?? ""}
        scopeName={members?.name ?? ""}
        open={members !== null}
        onOpenChange={(open) => { if (!open) setMembers(null) }}
      />
    </>
  )
}

// ----------------------------------------------------------- permissions --

export function PermissionsPage() {
  const { t } = useTranslation()
  const users = useQuery({ queryKey: ["admin", "/users", "directory"], queryFn: () => directory(admin.users) })
  const keys = useQuery({ queryKey: ["admin", "/api-keys", "directory"], queryFn: () => directory(admin.apiKeys) })
  const channels = useQuery({ queryKey: ["admin", "/providers", "directory"], queryFn: () => directory(providers) })
  const modelList = useQuery({ queryKey: ["admin", "/models", "directory"], queryFn: () => directory(models) })
  const fields: FormField[] = [
    { name: "action", label: t("fields.permissionEffect"), kind: "select", options: ACTIONS, required: true },
    { name: "modelPattern", kind: "searchable", allowCustom: true, emptyValue: "*", emptyLabel: t("form.all"), choices: (modelList.data ?? []).map(row => ({ value: row.name, label: row.name })) },
    { name: "userId", kind: "searchable", nullable: true, emptyLabel: t("form.all"), choices: (users.data ?? []).map(row => ({ value: row.id, label: `${row.name} (${row.id})` })) },
    { name: "apiKeyId", kind: "searchable", nullable: true, emptyLabel: t("form.all"), choices: (keys.data ?? []).map(row => ({ value: row.id, label: `${row.name} (${row.prefix})` })) },
    { name: "providerId", kind: "searchable", nullable: true, emptyLabel: t("form.all"), choices: (channels.data ?? []).map(row => ({ value: row.id, label: `${row.name} (${row.id})` })) },
    { name: "operation", kind: "text", nullable: true },
    { name: "priority", kind: "number" },
  ]
  return (
    <CollectionPage
      id="permissions"
      family={admin.permissions}
      rowId={(row: PermissionDto) => row.id}
      rowLabel={(row) => row.modelPattern}
      columns={[
        { key: "action", header: t("fields.permissionEffect"), cell: (row) => t(`values.${row.action}`) },
        { key: "modelPattern", cell: (row) => <IdCell value={row.modelPattern} /> },
        { key: "userId", cell: (row) => <MaybeCell value={row.userId} mono /> },
        { key: "apiKeyId", cell: (row) => <MaybeCell value={row.apiKeyId} mono /> },
        { key: "providerId", cell: (row) => <MaybeCell value={row.providerId} mono /> },
        { key: "operation", cell: (row) => <MaybeCell value={row.operation} /> },
        { key: "priority", cell: (row) => row.priority },
      ]}
      fields={fields}
      renderForm={props => <RecordDialog {...props} mode={props.original ? "edit" : "create"} title={t(props.original ? "edit.permissions" : "create.permissions")} fields={fields} error={props.error ?? users.error ?? keys.error ?? channels.error ?? modelList.error} /> }
    />
  )
}

// ----------------------------------------------------------- rate limits --

export function RateLimitsPage() {
  return (
    <CollectionPage
      id="rate-limits"
      family={admin.rateLimits}
      rowId={(row: RateLimitDto) => row.id}
      rowLabel={(row) => row.metric}
      columns={[
        { key: "metric", cell: (row) => row.metric },
        { key: "limitValue", cell: (row) => <IdCell value={row.limitValue} /> },
        { key: "periodSeconds", cell: (row) => row.periodSeconds },
        { key: "userId", cell: (row) => <MaybeCell value={row.userId} mono /> },
        { key: "apiKeyId", cell: (row) => <MaybeCell value={row.apiKeyId} mono /> },
        { key: "modelPattern", cell: (row) => <MaybeCell value={row.modelPattern} mono /> },
        { key: "enabled", cell: (row) => <BoolCell value={row.enabled} /> },
      ]}
      fields={[
        { name: "metric", kind: "text", required: true },
        { name: "limitValue", kind: "text", required: true },
        { name: "periodSeconds", kind: "number", required: true },
        { name: "userId", kind: "text", nullable: true },
        { name: "apiKeyId", kind: "text", nullable: true },
        { name: "modelPattern", kind: "text", nullable: true },
        { name: "enabled", kind: "switch" },
      ]}
    />
  )
}

// --------------------------------------------------------- oauth clients --
export function OAuthClientsPage() {
  const { t } = useTranslation()
  const client = useQueryClient()
  const retire = useMutation({
    mutationFn: (id: string) => admin.oauthClients.retire(id),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: ["admin", admin.oauthClients.path] })
      toast.success(t("toast.retired"))
    },
    onError: (error: Error) => toast.error(error.message),
  })
  return (
    <CollectionPage
      id="oauth-clients"
      family={admin.oauthClients}
      searchable
      // A client is retired, never deleted: the grants it issued still name it.
      deletable={false}
      rowId={(row: OAuthClientDto) => row.id}
      rowLabel={(row) => row.name}
      columns={[
        { key: "name", cell: (row) => row.name },
        { key: "id", cell: (row) => <IdCell value={row.id} /> },
        { key: "redirectUris", cell: (row) => <IdCell value={row.redirectUris.join(", ") || "—"} /> },
        { key: "enabled", cell: (row) => <BoolCell value={row.enabled} /> },
        { key: "deletedAtMs", cell: (row) => <InstantCell value={row.deletedAtMs} /> },
      ]}
      fields={[
        // The id *is* the `client_id` a third-party binary was built with, so
        // it is required rather than generated, and immutable afterwards.
        { name: "id", kind: "text", required: true, createOnly: true },
        { name: "name", kind: "text", required: true },
        { name: "redirectUris", kind: "lines" },
        { name: "enabled", kind: "switch" },
      ]}
      rowActions={(row) =>
        row.deletedAtMs === null ? (
          <ConfirmButton
            title={t("confirm.retireTitle", { name: row.name })}
            confirmLabel={t("actions.retire")}
            onConfirm={() => retire.mutate(row.id)}
          >
            {t("actions.retire")}
          </ConfirmButton>
        ) : null
      }
    />
  )
}

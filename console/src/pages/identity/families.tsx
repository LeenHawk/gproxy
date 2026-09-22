//! The identity families, each one a declaration.
//!
//! Twelve pages, twelve declarations, one [`CollectionPage`]. A family that
//! needs more than the five routes says so in `rowActions` — a key's rotate
//! and reveal, a client's retire — and nothing else here is bespoke.
//!
//! The field lists are the DTOs' write and patch shapes, in the DTO's order.
//! Where a column is immutable the field is `createOnly` (a team's
//! organization, a subscription's user), and where it is nullable the field is
//! `nullable`, which is what makes an emptied input send the `null` that
//! `double_option` turns into "clear it".

import { useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import * as admin from "@/api/admin"
import { BoolCell, InstantCell, MaybeCell } from "@/components/cells"
import { ConfirmButton } from "@/components/confirm"
import { IdCell } from "@/components/data-table"
import { SecretDialog } from "@/components/secret-dialog"
import { Button } from "@/components/ui/button"
import type {
  ApiKeyCreated, ApiKeyDto, OAuthClientDto, OrganizationDto, PermissionDto, PlanDto,
  PoolDto, RateLimitDto, SubscriptionDto, TeamDto, UserDto,
} from "@/generated/app"
import { CollectionPage } from "@/pages/identity/collection"
import { MembersDialog } from "@/pages/identity/members"

const ROLES = ["user", "admin"] as const
const ACTIONS = ["allow", "deny"] as const
const PERIODS = ["total", "fixed", "day", "week", "month"] as const

// ----------------------------------------------------------------- users --

export function UsersPage() {
  return (
    <CollectionPage
      id="users"
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
  )
}

// -------------------------------------------------------------- api keys --

export function ApiKeysPage() {
  const { t } = useTranslation()
  const client = useQueryClient()
  const [token, setToken] = useState<string | null>(null)
  const invalidate = () => client.invalidateQueries({ queryKey: ["admin", admin.apiKeys.path] })

  const rotate = useMutation({
    mutationFn: (id: string) => admin.apiKeys.rotate(id),
    onSuccess: (created) => { setToken(created.token); void invalidate() },
    onError: (error: Error) => toast.error(error.message),
  })
  const reveal = useMutation({
    mutationFn: (id: string) => admin.apiKeys.reveal(id),
    onSuccess: (secret) => setToken(secret.token),
    onError: (error: Error) => toast.error(error.message),
  })

  return (
    <>
      <CollectionPage
        id="api-keys"
        family={admin.apiKeys}
        searchable
        create={(body) => admin.createApiKey(body as Parameters<typeof admin.createApiKey>[0])}
        onCreated={(created) => setToken((created as ApiKeyCreated).token)}
        rowId={(row: ApiKeyDto) => row.id}
        rowLabel={(row) => row.name}
        columns={[
          { key: "name", cell: (row) => row.name },
          { key: "prefix", cell: (row) => <IdCell value={row.prefix} /> },
          { key: "kind", cell: (row) => <MaybeCell value={row.kind} /> },
          { key: "userId", cell: (row) => <IdCell value={row.userId} /> },
          { key: "enabled", cell: (row) => <BoolCell value={row.enabled} /> },
          { key: "expiresAtMs", cell: (row) => <InstantCell value={row.expiresAtMs} /> },
          { key: "hasSecret", cell: (row) => <BoolCell value={row.hasSecret} /> },
        ]}
        fields={[
          { name: "userId", kind: "text", required: true, createOnly: true },
          { name: "name", kind: "text", required: true },
          { name: "organizationId", kind: "text", nullable: true },
          { name: "teamId", kind: "text", nullable: true },
          { name: "subscriptionId", kind: "text", nullable: true },
          { name: "expiresAtMs", kind: "datetime", nullable: true },
          { name: "enabled", kind: "switch" },
          { name: "retainSecret", kind: "switch", createOnly: true },
        ]}
        rowActions={(row) => (
          <>
            {row.hasSecret ? (
              <Button variant="ghost" size="sm" onClick={() => reveal.mutate(row.id)}>{t("actions.reveal")}</Button>
            ) : null}
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
      <SecretDialog token={token} onClose={() => setToken(null)} />
    </>
  )
}

// --------------------------------------------------------- organizations --

export function OrganizationsPage() {
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
          <Button variant="ghost" size="sm" onClick={() => setMembers(row)}>{t("actions.members")}</Button>
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
          <Button variant="ghost" size="sm" onClick={() => setMembers(row)}>{t("actions.members")}</Button>
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
  return (
    <CollectionPage
      id="permissions"
      family={admin.permissions}
      rowId={(row: PermissionDto) => row.id}
      rowLabel={(row) => row.modelPattern}
      columns={[
        { key: "action", cell: (row) => <MaybeCell value={row.action} /> },
        { key: "modelPattern", cell: (row) => <IdCell value={row.modelPattern} /> },
        { key: "userId", cell: (row) => <MaybeCell value={row.userId} mono /> },
        { key: "apiKeyId", cell: (row) => <MaybeCell value={row.apiKeyId} mono /> },
        { key: "providerId", cell: (row) => <MaybeCell value={row.providerId} mono /> },
        { key: "operation", cell: (row) => <MaybeCell value={row.operation} /> },
        { key: "priority", cell: (row) => row.priority },
      ]}
      fields={[
        { name: "action", kind: "select", options: ACTIONS, required: true },
        { name: "modelPattern", kind: "text" },
        { name: "userId", kind: "text", nullable: true },
        { name: "apiKeyId", kind: "text", nullable: true },
        { name: "providerId", kind: "text", nullable: true },
        { name: "operation", kind: "text", nullable: true },
        { name: "priority", kind: "number" },
      ]}
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

// ----------------------------------------------------------------- plans --

export function PlansPage() {
  return (
    <CollectionPage
      id="plans"
      family={admin.plans}
      searchable
      rowId={(row: PlanDto) => row.id}
      rowLabel={(row) => row.name}
      columns={[
        { key: "name", cell: (row) => row.name },
        { key: "poolId", cell: (row) => <IdCell value={row.poolId} /> },
        { key: "codexPlanType", cell: (row) => <MaybeCell value={row.codexPlanType} /> },
        { key: "claudeSubscriptionType", cell: (row) => <MaybeCell value={row.claudeSubscriptionType} /> },
        { key: "enabled", cell: (row) => <BoolCell value={row.enabled} /> },
        { key: "id", cell: (row) => <IdCell value={row.id} /> },
      ]}
      fields={[
        { name: "poolId", kind: "text", required: true },
        { name: "name", kind: "text", required: true },
        { name: "codexPlanType", kind: "text", nullable: true },
        { name: "claudeSubscriptionType", kind: "text", nullable: true },
        { name: "claudeRateLimitTier", kind: "text", nullable: true },
        { name: "enabled", kind: "switch" },
      ]}
    />
  )
}

/** A plan's allowances. Its own family, as `/admin/api/plan-limits` is. */
export function PlanLimitsPage() {
  return (
    <CollectionPage
      id="plan-limits"
      family={admin.planLimits}
      rowId={(row) => row.id}
      rowLabel={(row) => row.windowKey}
      columns={[
        { key: "planId", cell: (row) => <IdCell value={row.planId} /> },
        { key: "windowKey", cell: (row) => row.windowKey },
        { key: "limit", cell: (row) => <IdCell value={row.limit} /> },
        { key: "period", cell: (row) => <MaybeCell value={row.period} /> },
        { key: "periodSeconds", cell: (row) => <MaybeCell value={row.periodSeconds?.toString() ?? null} /> },
        { key: "modelPattern", cell: (row) => <MaybeCell value={row.modelPattern} mono /> },
      ]}
      fields={[
        { name: "planId", kind: "text", required: true, createOnly: true },
        { name: "windowKey", kind: "text", required: true },
        { name: "limit", kind: "text", required: true },
        { name: "period", kind: "select", options: PERIODS, required: true },
        { name: "periodSeconds", kind: "number", nullable: true },
        { name: "modelPattern", kind: "text", nullable: true },
      ]}
    />
  )
}

// --------------------------------------------------------- subscriptions --

export function SubscriptionsPage() {
  return (
    <CollectionPage
      id="subscriptions"
      family={admin.subscriptions}
      rowId={(row: SubscriptionDto) => row.id}
      rowLabel={(row) => row.id}
      columns={[
        { key: "userId", cell: (row) => <IdCell value={row.userId} /> },
        { key: "planId", cell: (row) => <IdCell value={row.planId} /> },
        { key: "enabled", cell: (row) => <BoolCell value={row.enabled} /> },
        { key: "startsAtMs", cell: (row) => <InstantCell value={row.startsAtMs} /> },
        { key: "expiresAtMs", cell: (row) => <InstantCell value={row.expiresAtMs} /> },
        { key: "id", cell: (row) => <IdCell value={row.id} /> },
      ]}
      fields={[
        { name: "userId", kind: "text", required: true, createOnly: true },
        { name: "planId", kind: "text", required: true },
        { name: "enabled", kind: "switch" },
        { name: "startsAtMs", kind: "datetime" },
        { name: "expiresAtMs", kind: "datetime", nullable: true },
      ]}
    />
  )
}

// ----------------------------------------------------------------- pools --

export function PoolsPage() {
  return (
    <CollectionPage
      id="pools"
      family={admin.pools}
      searchable
      rowId={(row: PoolDto) => row.id}
      rowLabel={(row) => row.name}
      columns={[
        { key: "name", cell: (row) => row.name },
        { key: "enabled", cell: (row) => <BoolCell value={row.enabled} /> },
        { key: "createdAtMs", cell: (row) => <InstantCell value={row.createdAtMs} /> },
        { key: "id", cell: (row) => <IdCell value={row.id} /> },
      ]}
      fields={[
        { name: "name", kind: "text", required: true },
        { name: "enabled", kind: "switch" },
      ]}
    />
  )
}

/**
 * The upstream subscriptions a pool draws on. `sourceKey` is the canonical
 * issuer plus upstream subscription identity, which is what deduplicates one
 * real subscription imported under several credentials.
 */
export function PoolMembersPage() {
  return (
    <CollectionPage
      id="pool-members"
      family={admin.poolMembers}
      rowId={(row) => row.id}
      rowLabel={(row) => row.sourceKey}
      columns={[
        { key: "poolId", cell: (row) => <IdCell value={row.poolId} /> },
        { key: "credentialId", cell: (row) => <IdCell value={row.credentialId} /> },
        { key: "sourceKey", cell: (row) => <IdCell value={row.sourceKey} /> },
        { key: "enabled", cell: (row) => <BoolCell value={row.enabled} /> },
      ]}
      fields={[
        { name: "poolId", kind: "text", required: true },
        { name: "credentialId", kind: "text", required: true },
        { name: "sourceKey", kind: "text", required: true },
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

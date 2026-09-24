import { KeyCreateDialog } from "@/components/keys/create-dialog"
import { KeySettingsDialog } from "@/components/keys/settings-dialog"
import { createApiKey } from "@/api/admin"
import type { ApiKeyWrite, PortalKeyDto } from "@/generated/app"
//! The caller's own gateway keys.
//!
//! What a key is bound to — an organization or a team — decides
//! the budget chain, the permission subject and which credentials it may reach,
//! all at once and at mint time. It is never taken from a request header, so
//! the choice is made here or not at all; the two selects are populated from
//! the caller's own memberships, because those are the only bindings the server
//! will accept.

import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { Plus } from "lucide-react"
import { toast } from "sonner"
import * as portal from "@/api/portal"
import { useConsoleContext } from "@/capability/session"
import { SELF_KEYS_WRITE } from "@/capability/capability"
import { BoolCell, InstantCell, MaybeCell } from "@/components/cells"
import { ConfirmButton } from "@/components/confirm"
import { DataTable } from "@/components/data-table"
import { Page, PageHeader } from "@/components/page"
import { type FormField } from "@/components/record-form"
import { KeySecretCell } from "@/components/key-secret-cell"
import { SecretDialog } from "@/components/secret-dialog"
import { EmptyNotice, QueryState } from "@/components/state"
import { Alert, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"

const KEYS = ["portal", "keys"] as const

export function KeysPage() {
  const { t } = useTranslation()
  const client = useQueryClient()
  const context = useConsoleContext()
  const maySetBudget = context.has("identity.api-keys.write") && context.has("configuration.quotas.write")
  const [editing, setEditing] = useState<{ row: PortalKeyDto; tab: "basic" | "budget" } | null>(null)
  const mayWrite = context.has(SELF_KEYS_WRITE)
  const [creating, setCreating] = useState(false)
  const [token, setToken] = useState<string | null>(null)

  const list = useQuery({ queryKey: KEYS, queryFn: portal.keys.list })
  const invalidate = () => client.invalidateQueries({ queryKey: KEYS })
  const fail = (error: Error) => toast.error(error.message)

  const create = useMutation({
    mutationFn: (body: Record<string, unknown>) => maySetBudget ? createApiKey({ ...body, userId: context.userId } as ApiKeyWrite) : portal.keys.create(body as Parameters<typeof portal.keys.create>[0]),
    onSuccess: (created) => { setCreating(false); setToken(created.token); void invalidate(); void client.invalidateQueries({ queryKey: ["admin", "/api-keys"] }); void client.invalidateQueries({ queryKey: ["admin", "/quotas"] }) },
  })
  const rotate = useMutation({
    mutationFn: portal.keys.rotate,
    onSuccess: (created) => { setToken(created.token); void invalidate() },
    onError: fail,
  })
  const remove = useMutation({
    mutationFn: portal.keys.remove,
    onSuccess: () => { void invalidate(); toast.success(t("toast.deleted")) },
    onError: fail,
  })

  const fields: Array<FormField> = [
    { name: "name", kind: "text", required: true },
    { name: "expiresAtMs", kind: "datetime", nullable: true },
    ...(context.organizations.length
      ? [{
        name: "organizationId",
        kind: "select" as const,
        nullable: true,
        choices: context.organizations.map((entry) => ({ value: entry.id, label: entry.name })),
      }]
      : []),
    ...(context.teams.length
      ? [{
        name: "teamId",
        kind: "select" as const,
        nullable: true,
        choices: context.teams.map((entry) => ({ value: entry.id, label: entry.name })),
      }]
      : []),
    { name: "retainSecret", kind: "switch" },
  ]

  return (
    <Page>
      <PageHeader
        title={t("nav.keys")}
        actions={
          mayWrite ? (
            <Button size="sm" onClick={() => setCreating(true)}><Plus /> {t("actions.new")}</Button>
          ) : null
        }
      />
      {mayWrite ? null : (
        <Alert>
          <AlertTitle>{t("keys.readOnlyTitle")}</AlertTitle>
        </Alert>
      )}
      <QueryState isPending={list.isPending} error={list.error}>
        <DataTable paginate
          columns={[
            { key: "name", cell: (row) => row.name },
            { key: "prefix", header: t("keys.value"), cell: (row) => <KeySecretCell key={`${row.id}:${row.prefix}`} prefix={row.prefix} revealable={row.hasSecret} reveal={() => portal.keys.reveal(row.id)} /> },
            { key: "organizationId", cell: (row) => <MaybeCell value={row.organizationId} mono /> },
            { key: "teamId", cell: (row) => <MaybeCell value={row.teamId} mono /> },
            { key: "expiresAtMs", cell: (row) => <InstantCell value={row.expiresAtMs} /> },
            { key: "enabled", cell: (row) => <BoolCell value={row.enabled} /> },
          ]}
          rows={list.data ?? []}
          rowKey={(row) => row.id}
          empty={<EmptyNotice title={t("keys.emptyTitle")} />}
          actions={(row) => (
            <>
              {maySetBudget ? <Button size="sm" variant="ghost" onClick={() => setEditing({ row, tab: "basic" })}>{t("actions.edit")}</Button> : null}
              {mayWrite ? (
                <>
                  <ConfirmButton
                    title={t("confirm.rotateTitle", { name: row.name })}
                    confirmLabel={t("actions.rotate")}
                    onConfirm={() => rotate.mutate(row.id)}
                  >
                    {t("actions.rotate")}
                  </ConfirmButton>
                  <ConfirmButton
                    title={t("confirm.deleteTitle", { name: row.name })}
                    onConfirm={() => remove.mutate(row.id)}
                  >
                    {t("actions.delete")}
                  </ConfirmButton>
                </>
              ) : null}
            </>
          )}
        />
      </QueryState>

      <KeyCreateDialog
        canSetBudget={maySetBudget}
        open={creating}
        onOpenChange={setCreating}
        title={t("create.keys")}
        fields={fields}
        onSubmit={(body) => create.mutate(body)}
        pending={create.isPending}
        error={create.error}
      />
      {editing ? <KeySettingsDialog apiKey={editing.row} fields={[...fields.filter(field => field.name !== "retainSecret"), { name: "enabled", kind: "switch" }]} initialTab={editing.tab} onClose={() => setEditing(null)} /> : null}
      <SecretDialog token={token} onClose={() => setToken(null)} />
    </Page>
  )
}

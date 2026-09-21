//! The caller's own gateway keys.
//!
//! What a key is bound to — an organization, a team, a subscription — decides
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
import { DataTable, IdCell } from "@/components/data-table"
import { Page, PageHeader } from "@/components/page"
import { RecordDialog, type FormField } from "@/components/record-form"
import { SecretDialog } from "@/components/secret-dialog"
import { EmptyNotice, QueryState } from "@/components/state"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"

const KEYS = ["portal", "keys"] as const

export function KeysPage() {
  const { t } = useTranslation()
  const client = useQueryClient()
  const context = useConsoleContext()
  const mayWrite = context.has(SELF_KEYS_WRITE)
  const [creating, setCreating] = useState(false)
  const [token, setToken] = useState<string | null>(null)

  const list = useQuery({ queryKey: KEYS, queryFn: portal.keys.list })
  const invalidate = () => client.invalidateQueries({ queryKey: KEYS })
  const fail = (error: Error) => toast.error(error.message)

  const create = useMutation({
    mutationFn: (body: Record<string, unknown>) => portal.keys.create(body as Parameters<typeof portal.keys.create>[0]),
    onSuccess: (created) => { setCreating(false); setToken(created.token); void invalidate() },
  })
  const rotate = useMutation({
    mutationFn: portal.keys.rotate,
    onSuccess: (created) => { setToken(created.token); void invalidate() },
    onError: fail,
  })
  const reveal = useMutation({
    mutationFn: portal.keys.reveal,
    onSuccess: (secret) => setToken(secret.token),
    onError: fail,
  })
  const remove = useMutation({
    mutationFn: portal.keys.remove,
    onSuccess: () => { void invalidate(); toast.success(t("toast.deleted")) },
    onError: fail,
  })

  const fields: Array<FormField> = [
    { name: "name", kind: "text", required: true },
    { name: "expiresAtMs", kind: "datetime", help: true },
    ...(context.organizations.length
      ? [{
        name: "organizationId",
        kind: "select" as const,
        help: true,
        choices: context.organizations.map((entry) => ({ value: entry.id, label: entry.name })),
      }]
      : []),
    ...(context.teams.length
      ? [{
        name: "teamId",
        kind: "select" as const,
        choices: context.teams.map((entry) => ({ value: entry.id, label: entry.name })),
      }]
      : []),
    { name: "retainSecret", kind: "switch", help: true },
  ]

  return (
    <Page>
      <PageHeader
        title={t("nav.keys")}
        description={t("description.keys")}
        actions={
          mayWrite ? (
            <Button size="sm" onClick={() => setCreating(true)}><Plus /> {t("actions.new")}</Button>
          ) : null
        }
      />
      {mayWrite ? null : (
        <Alert>
          <AlertTitle>{t("keys.readOnlyTitle")}</AlertTitle>
          <AlertDescription>{t("keys.readOnlyDescription")}</AlertDescription>
        </Alert>
      )}
      <QueryState isPending={list.isPending} error={list.error}>
        <DataTable
          columns={[
            { key: "name", cell: (row) => row.name },
            { key: "prefix", cell: (row) => <IdCell value={row.prefix} /> },
            { key: "organizationId", cell: (row) => <MaybeCell value={row.organizationId} mono /> },
            { key: "teamId", cell: (row) => <MaybeCell value={row.teamId} mono /> },
            { key: "expiresAtMs", cell: (row) => <InstantCell value={row.expiresAtMs} /> },
            { key: "enabled", cell: (row) => <BoolCell value={row.enabled} /> },
          ]}
          rows={list.data ?? []}
          rowKey={(row) => row.id}
          empty={<EmptyNotice title={t("keys.emptyTitle")} description={t("keys.emptyDescription")} />}
          actions={(row) => (
            <>
              {row.hasSecret ? (
                <Button variant="ghost" size="sm" onClick={() => reveal.mutate(row.id)}>{t("actions.reveal")}</Button>
              ) : null}
              {mayWrite ? (
                <>
                  <ConfirmButton
                    title={t("confirm.rotateTitle")}
                    description={t("confirm.rotateDescription", { name: row.name })}
                    confirmLabel={t("actions.rotate")}
                    onConfirm={() => rotate.mutate(row.id)}
                  >
                    {t("actions.rotate")}
                  </ConfirmButton>
                  <ConfirmButton
                    title={t("confirm.deleteTitle")}
                    description={t("confirm.deleteDescription", { name: row.name })}
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

      <RecordDialog
        open={creating}
        onOpenChange={setCreating}
        mode="create"
        title={t("create.keys")}
        fields={fields}
        onSubmit={(body) => create.mutate(body)}
        pending={create.isPending}
        error={create.error}
      />
      <SecretDialog token={token} onClose={() => setToken(null)} />
    </Page>
  )
}

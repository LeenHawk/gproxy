//! The account page: the password, the scopes, the sign-ins and the grants.
//!
//! Four things a person does to their own account, on one page because none of
//! them is big enough to be its own. The password change asks for the current
//! one **only when there is one**: an account that signs in through OAuth alone
//! has nothing to prove, and asking it for a password it never set would be
//! unanswerable.

import { useState, type FormEvent } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import * as portal from "@/api/portal"
import { SELF_PASSWORD_CHANGE } from "@/capability/capability"
import { useConsoleContext } from "@/capability/session"
import { InstantCell, MaybeCell } from "@/components/cells"
import { ConfirmButton } from "@/components/confirm"
import { DataTable, IdCell } from "@/components/data-table"
import { Page, PageHeader, PageSection } from "@/components/page"
import { EmptyNotice, ErrorNotice, QueryState } from "@/components/state"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Field, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"

function PasswordCard() {
  const { t } = useTranslation()
  const context = useConsoleContext()
  const [current, setCurrent] = useState("")
  const [next, setNext] = useState("")

  const change = useMutation({
    mutationFn: () => portal.changePassword({ current: context.hasPassword ? current : null, new: next }),
    onSuccess: () => {
      setCurrent("")
      setNext("")
      toast.success(t("account.passwordChanged"))
    },
  })

  const submit = (event: FormEvent) => {
    event.preventDefault()
    if (!next) return
    change.mutate()
  }

  return (
    <form className="max-w-sm space-y-3" onSubmit={submit}>
      {change.error ? <ErrorNotice error={change.error} /> : null}
      {/*
        A password form with no username field makes browsers (and Chrome's
        own console) complain, and makes a password manager save the entry
        under no account at all. The name is already known here, so it is
        given rather than asked for.
      */}
      <input
        type="text"
        name="username"
        autoComplete="username"
        value={context.userName}
        readOnly
        hidden
      />
      {context.hasPassword ? (
        <Field>
          <FieldLabel htmlFor="current-password">{t("fields.currentPassword")}</FieldLabel>
          <Input
            id="current-password"
            type="password"
            autoComplete="current-password"
            value={current}
            onChange={(event) => setCurrent(event.target.value)}
          />
        </Field>
      ) : null}
      <Field>
        <FieldLabel htmlFor="new-password">{t("fields.newPassword")}</FieldLabel>
        <Input
          id="new-password"
          type="password"
          autoComplete="new-password"
          value={next}
          onChange={(event) => setNext(event.target.value)}
        />
      </Field>
      <Button type="submit" size="sm" disabled={change.isPending || !next}>{t("actions.save")}</Button>
    </form>
  )
}

function ScopesCard() {
  const { t } = useTranslation()
  const context = useConsoleContext()
  if (context.organizations.length === 0 && context.teams.length === 0) {
    return <EmptyNotice title={t("account.noScopes")} />
  }
  return (
    <ul className="space-y-2">
      {context.organizations.map((entry) => (
        <li key={`organization:${entry.id}`} className="flex items-center gap-2 text-sm">
          <Badge variant="outline">{t("scope.organization")}</Badge>
          <span>{entry.name}</span>
          <Badge variant={entry.role === "admin" ? "info" : "secondary"}>{t(`values.${entry.role}`)}</Badge>
        </li>
      ))}
      {context.teams.map((entry) => (
        <li key={`team:${entry.id}`} className="flex items-center gap-2 text-sm">
          <Badge variant="outline">{t("scope.team")}</Badge>
          <span>{entry.name}</span>
          <Badge variant={entry.role === "admin" ? "info" : "secondary"}>{t(`values.${entry.role}`)}</Badge>
        </li>
      ))}
    </ul>
  )
}

function SessionsCard() {
  const { t } = useTranslation()
  const list = useQuery({ queryKey: ["portal", "sessions"], queryFn: portal.sessions })
  return (
    <QueryState isPending={list.isPending} error={list.error} rows={2}>
      <DataTable
        columns={[
          { key: "createdAtMs", cell: (row) => <InstantCell value={row.createdAtMs} /> },
          { key: "expiresAtMs", cell: (row) => <InstantCell value={row.expiresAtMs} /> },
          { key: "id", cell: (row) => <IdCell value={row.id} /> },
        ]}
        rows={list.data ?? []}
        rowKey={(row) => row.id}
        empty={<EmptyNotice title={t("state.emptyTitle")} />}
      />
    </QueryState>
  )
}

function GrantsCard() {
  const { t } = useTranslation()
  const client = useQueryClient()
  const key = ["portal", "oauth-sessions"] as const
  const list = useQuery({ queryKey: key, queryFn: portal.oauthSessions.list })
  const revoke = useMutation({
    mutationFn: portal.oauthSessions.revoke,
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: key })
      toast.success(t("toast.revoked"))
    },
    onError: (error: Error) => toast.error(error.message),
  })
  return (
    <QueryState isPending={list.isPending} error={list.error} rows={2}>
      <DataTable
        columns={[
          { key: "clientName", cell: (row) => <MaybeCell value={row.clientName ?? row.clientId} /> },
          {
            key: "scopes",
            cell: (row) => (
              <span className="flex flex-wrap gap-1">
                {row.scopes.map((scope) => <Badge key={scope} variant="outline">{scope}</Badge>)}
              </span>
            ),
          },
          { key: "createdAtMs", cell: (row) => <InstantCell value={row.createdAtMs} /> },
          { key: "loggedInAtMs", cell: (row) => <InstantCell value={row.loggedInAtMs} /> },
          { key: "lastRefreshedAtMs", cell: (row) => <InstantCell value={row.lastRefreshedAtMs} /> },
          { key: "refreshCount", cell: (row) => row.refreshCount },
        ]}
        rows={list.data ?? []}
        rowKey={(row) => row.id}
        empty={<EmptyNotice title={t("account.noGrants")} />}
        actions={(row) => (
          <ConfirmButton
            title={t("confirm.revokeGrantTitle", { name: row.clientName ?? row.clientId })}
            confirmLabel={t("actions.revoke")}
            onConfirm={() => revoke.mutate(row.id)}
          >
            {t("actions.revoke")}
          </ConfirmButton>
        )}
      />
    </QueryState>
  )
}

export function AccountPage() {
  const { t } = useTranslation()
  const context = useConsoleContext()
  return (
    <Page>
      <PageHeader title={t("nav.account")} />
      {context.has(SELF_PASSWORD_CHANGE) ? (
        <PageSection title={t("account.password")}>
          <PasswordCard />
        </PageSection>
      ) : null}
      <PageSection title={t("account.scopes")}>
        <ScopesCard />
      </PageSection>
      <PageSection title={t("account.sessions")}>
        <SessionsCard />
      </PageSection>
      <PageSection title={t("account.grants")}>
        <GrantsCard />
      </PageSection>
    </Page>
  )
}

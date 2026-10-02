//! Membership, which is the only writable thing about a scope that is not a
//! column on it.
//!
//! Both membership tables have a composite primary key and no surrogate id, so
//! a member is addressed by the pair and moving one between scopes is a remove
//! and an add rather than a patch — the pair *is* the key. The two DTOs differ
//! only in which half they name (`organizationId` or `teamId`) and this
//! component reads neither: the scope comes from the dialog it was opened on.
//!
//! A per-scope `role` of `admin` is what the capability seam turns into an
//! [`AdminScope`](@/capability/capability). Today that grants no
//! `identity.*` capability, because `/admin/api` is instance-only; this is
//! where an operator sets it up in advance of the scoped routes.

import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { organizationMembers, teamMembers } from "@/api/admin"
import { ConfirmButton } from "@/components/confirm"
import { DataTable, IdCell } from "@/components/data-table"
import { EmptyNotice, QueryState } from "@/components/state"
import { Button } from "@/components/ui/button"
import {
  Dialog, DialogBody, DialogContent, DialogHeader, DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"

export type MemberScope = "organization" | "team"

/**
 * The half of a membership row this page reads.
 *
 * `OrganizationMemberDto` and `TeamMemberDto` differ only in which id they
 * name, and the scope is already known from the dialog that was opened — so
 * the two are narrowed to their common half rather than branched on.
 */
type MemberRow = { userId: string; role: string }

const ROLES = ["member", "admin"] as const

export function MembersDialog({ scope, scopeId, scopeName, open, onOpenChange }: {
  scope: MemberScope
  scopeId: string
  scopeName: string
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const [userId, setUserId] = useState("")
  const [role, setRole] = useState<string>("member")

  const key = ["admin", "members", scope, scopeId] as const
  const list = useQuery({
    queryKey: key,
    enabled: open,
    queryFn: async (): Promise<Array<MemberRow>> => {
      const page = scope === "organization"
        ? await organizationMembers.list(scopeId)
        : await teamMembers.list(scopeId)
      return page.items.map((row) => ({ userId: row.userId, role: row.role }))
    },
  })
  const invalidate = () => client.invalidateQueries({ queryKey: key })
  const fail = (error: Error) => toast.error(error.message)

  const add = useMutation({
    mutationFn: async () => {
      const write = { userId: userId.trim(), role }
      if (scope === "organization") await organizationMembers.add(scopeId, write)
      else await teamMembers.add(scopeId, write)
    },
    onSuccess: () => { setUserId(""); void invalidate(); toast.success(t("toast.added")) },
    onError: fail,
  })
  const setRoleOf = useMutation({
    mutationFn: async ({ member, next }: { member: string; next: string }) => {
      if (scope === "organization") await organizationMembers.setRole(scopeId, member, { role: next })
      else await teamMembers.setRole(scopeId, member, { role: next })
    },
    onSuccess: () => { void invalidate(); toast.success(t("toast.saved")) },
    onError: fail,
  })
  const remove = useMutation({
    mutationFn: async (member: string) => {
      if (scope === "organization") await organizationMembers.remove(scopeId, member)
      else await teamMembers.remove(scopeId, member)
    },
    onSuccess: () => { void invalidate(); toast.success(t("toast.removed")) },
    onError: fail,
  })

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent aria-describedby={undefined} className="sm:max-w-2xl" closeLabel={t("actions.close")}>
        <DialogHeader>
          <DialogTitle>{t("members.title")} · {scopeName}</DialogTitle>
        </DialogHeader>
        <DialogBody className="space-y-4">
          <div className="flex flex-wrap items-center gap-2">
            <Input
              className="max-w-xs"
              aria-label={t("fields.userId")} placeholder={t("fields.userId")}
              value={userId}
              onChange={(event) => setUserId(event.target.value)}
            />
            <Select value={role} onValueChange={setRole}>
              <SelectTrigger aria-label={t("fields.role")} className="w-36"><SelectValue /></SelectTrigger>
              <SelectContent>
                {ROLES.map((option) => <SelectItem key={option} value={option}>{t(`values.${option}`)}</SelectItem>)}
              </SelectContent>
            </Select>
            <Button size="sm" disabled={!userId.trim() || add.isPending} onClick={() => add.mutate()}>
              {t("actions.add")}
            </Button>
          </div>
          <QueryState isPending={list.isPending} error={list.error}>
            <DataTable storageKey="group-members" paginate
              columns={[
                { key: "userId", cell: (row) => <IdCell value={row.userId} /> },
                {
                  key: "role",
                  cell: (row) => (
                    <Select
                      value={row.role}
                      onValueChange={(next) => setRoleOf.mutate({ member: row.userId, next })}
                    >
                      <SelectTrigger aria-label={`${t("fields.role")}: ${row.userId}`} className="w-32"><SelectValue /></SelectTrigger>
                      <SelectContent>
                        {ROLES.map((option) => (
                          <SelectItem key={option} value={option}>{t(`values.${option}`)}</SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  ),
                },
              ]}
              rows={list.data ?? []}
              rowKey={(row) => row.userId}
              empty={<EmptyNotice title={t("members.emptyTitle")} />}
              actions={(row) => (
                <ConfirmButton
                  title={t("confirm.removeMemberTitle", { name: row.userId })}
                  confirmLabel={t("actions.remove")}
                  onConfirm={() => remove.mutate(row.userId)}
                >
                  {t("actions.remove")}
                </ConfirmButton>
              )}
            />
          </QueryState>
        </DialogBody>
      </DialogContent>
    </Dialog>
  )
}

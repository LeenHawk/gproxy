import { useRef, useState, type MouseEvent } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { createUserKey, revealUserKey, updateUserKey } from "@/api/identity"
import type { UserDto } from "@/generated/UserDto"
import type { UserKeyCreateResponse } from "@/generated/UserKeyCreateResponse"
import type { UserKeyDto } from "@/generated/UserKeyDto"
import type { AccessManagerProps } from "@/components/keys/access-manager"
import { ScopeAccessEditor } from "@/components/keys/access-manager"
import { CreatedKeyDialog } from "@/components/keys/created-key-dialog"
import { KeyForm } from "@/components/keys/key-form"
import { KeyTable } from "@/components/keys/key-table"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Dialog } from "@/components/ui/dialog"
import { Empty, EmptyContent, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from "@/components/ui/sheet"

export function UserKeyPanel({ user, access }: { user: UserDto; access: AccessManagerProps }) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const opener = useRef<HTMLButtonElement | null>(null)
  const [formOpen, setFormOpen] = useState(false)
  const [created, setCreated] = useState<UserKeyCreateResponse | null>(null)
  const [accessKey, setAccessKey] = useState<UserKeyDto | null>(null)
  const keys = access.keys.filter((key) => key.user_id === user.id)
  const create = useMutation({
    mutationFn: createUserKey,
    onSuccess: async (value) => { setFormOpen(false); setCreated(value); await client.invalidateQueries({ queryKey: ["user-keys"] }) },
    onError: () => toast.error(t("users.keys.createError")),
  })
  const update = useMutation({
    mutationFn: (key: UserKeyDto) => updateUserKey(key.id, { label: key.label, expires_at: key.expires_at, enabled: !key.enabled }),
    onSuccess: async () => { toast.success(t("users.keys.updated")); await client.invalidateQueries({ queryKey: ["user-keys"] }) },
    onError: () => toast.error(t("users.keys.updateError")),
  })
  const open = (event: MouseEvent<HTMLButtonElement>) => { opener.current = event.currentTarget; setFormOpen(true) }
  const returnFocus = () => opener.current?.focus()
  return (
    <>
      <Card>
        <CardHeader>
          <CardTitle>{t("users.keys.title")}</CardTitle>
          <CardDescription>{t("users.keys.scopedDescription", { user: user.name })}</CardDescription>
          <CardAction><Button ref={opener} size="sm" onClick={open}>{t("users.keys.create")}</Button></CardAction>
        </CardHeader>
        <CardContent>
          {keys.length ? <KeyTable keys={keys} users={[user]} showUser={false} pending={update.isPending} reveal={revealUserKey} onEnabledChange={update.mutate} onAccess={setAccessKey} /> : <Empty><EmptyHeader><EmptyTitle>{t("users.keys.empty")}</EmptyTitle></EmptyHeader><EmptyContent><Button onClick={open}>{t("users.keys.create")}</Button></EmptyContent></Empty>}
        </CardContent>
      </Card>
      <Dialog open={formOpen} onOpenChange={setFormOpen}><KeyForm users={[user]} user={user} pending={create.isPending} returnFocus={returnFocus} onSubmit={(value) => create.mutateAsync({ ...value, user_id: user.id }).then(() => undefined)} /></Dialog>
      <CreatedKeyDialog value={created} onClose={() => setCreated(null)} returnFocus={returnFocus} />
      <Sheet open={accessKey != null} onOpenChange={(open) => { if (!open) setAccessKey(null) }}>
        <SheetContent className="overflow-y-auto sm:max-w-3xl" closeLabel={t("common.actions.close")}>
          <SheetHeader><SheetTitle>{t("users.keys.accessTitle", { key: accessKey?.label ?? accessKey?.prefix ?? "" })}</SheetTitle><SheetDescription>{t("access.scope.description")}</SheetDescription></SheetHeader>
          {accessKey ? <div className="px-4 pb-4"><ScopeAccessEditor {...access} scope="user_key" scopeId={accessKey.id} /></div> : null}
        </SheetContent>
      </Sheet>
    </>
  )
}

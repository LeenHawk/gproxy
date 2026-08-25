import { useId, useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import type { UserDto } from "@/generated/UserDto"
import type { UserKeyCreateRequest } from "@/generated/UserKeyCreateRequest"
import type { UserKeyPrefix } from "@/generated/UserKeyPrefix"
import { Button } from "@/components/ui/button"
import { DialogClose, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"

export function KeyForm({ users, pending, onSubmit, returnFocus }: { users: Array<UserDto>; pending: boolean; onSubmit: (value: UserKeyCreateRequest) => Promise<void>; returnFocus: () => void }) {
  const { t } = useTranslation()
  const id = useId()
  const [userId, setUserId] = useState("")
  const [prefix, setPrefix] = useState<UserKeyPrefix>("sk")
  const [label, setLabel] = useState("")
  const [expiresAt, setExpiresAt] = useState("")

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    try {
      await onSubmit({
        user_id: Number(userId),
        prefix,
        label: label.trim() || null,
        expires_at: expiresAt ? Math.floor(new Date(expiresAt).getTime() / 1000) : null,
        enabled: true,
      })
    } catch {
      return
    }
  }

  return (
    <DialogContent showCloseButton={false} onCloseAutoFocus={(event) => { event.preventDefault(); returnFocus() }}>
      <DialogHeader><DialogTitle>{t("users.keys.create")}</DialogTitle></DialogHeader>
      <form className="flex flex-col gap-5" onSubmit={(event) => void submit(event)}>
        <FieldGroup>
          <Field>
            <FieldLabel htmlFor={`${id}-user`}>{t("access.subjectKinds.user")}</FieldLabel>
            <Select value={userId} onValueChange={setUserId}>
              <SelectTrigger id={`${id}-user`}><SelectValue placeholder={t("common.required")} /></SelectTrigger>
              <SelectContent><SelectGroup>{users.map((user) => <SelectItem key={user.id} value={String(user.id)}>{user.name}</SelectItem>)}</SelectGroup></SelectContent>
            </Select>
          </Field>
          <Field>
            <FieldLabel id={`${id}-prefix-label`}>{t("users.keys.prefix")}</FieldLabel>
            <ToggleGroup type="single" variant="outline" value={prefix} aria-labelledby={`${id}-prefix-label`} className="flex-wrap justify-start" onValueChange={(value) => { if (value) setPrefix(value as UserKeyPrefix) }}>
              <ToggleGroupItem value="sk">{t("users.keys.standardPrefix")}</ToggleGroupItem>
              <ToggleGroupItem value="at">{t("users.keys.codexPrefix")}</ToggleGroupItem>
            </ToggleGroup>
          </Field>
          <Field>
            <FieldLabel htmlFor={`${id}-label`}>{t("users.keys.label")}</FieldLabel>
            <Input id={`${id}-label`} value={label} onChange={(event) => setLabel(event.target.value)} />
          </Field>
          <Field>
            <FieldLabel htmlFor={`${id}-expires`}>{t("users.keys.expiresAt")}</FieldLabel>
            <Input id={`${id}-expires`} type="datetime-local" value={expiresAt} onChange={(event) => setExpiresAt(event.target.value)} />
          </Field>
        </FieldGroup>
        <DialogFooter>
          <DialogClose asChild><Button type="button" variant="outline">{t("common.actions.cancel")}</Button></DialogClose>
          <Button type="submit" disabled={pending || !userId}>{t(pending ? "common.actions.saving" : "common.actions.create")}</Button>
        </DialogFooter>
      </form>
    </DialogContent>
  )
}

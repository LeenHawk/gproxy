import { useId } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import type { UserKeyCreateResponse } from "@/generated/UserKeyCreateResponse"
import { Button } from "@/components/ui/button"
import { Dialog, DialogBody, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Field, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"

export function CreatedKeyDialog({ value, onClose, returnFocus }: { value: UserKeyCreateResponse | null; onClose: () => void; returnFocus: () => void }) {
  const { t } = useTranslation()
  const id = useId()

  const copy = async () => {
    if (!value) return
    try {
      await navigator.clipboard.writeText(value.api_key)
      toast.success(t("users.keys.copied"))
    } catch {
      toast.error(t("users.keys.copyError"))
    }
  }

  return (
    <Dialog open={value != null} onOpenChange={(open) => { if (!open) onClose() }}>
      <DialogContent showCloseButton={false} onCloseAutoFocus={(event) => { event.preventDefault(); returnFocus() }}>
        <DialogHeader><DialogTitle>{t("users.keys.created")}</DialogTitle></DialogHeader>
        <DialogBody><Field>
          <FieldLabel htmlFor={id}>{t("users.keys.title")}</FieldLabel>
          <Input id={id} className="font-mono" value={value?.api_key ?? ""} readOnly autoFocus />
        </Field></DialogBody>
        <DialogFooter>
          <Button type="button" variant="outline" onClick={onClose}>{t("common.actions.close")}</Button>
          <Button type="button" onClick={() => void copy()}>{t("users.keys.copy")}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

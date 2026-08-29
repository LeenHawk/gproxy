import type { ReactElement } from "react"
import { useState } from "react"
import { useTranslation } from "react-i18next"
import { type ProviderFormSource, useProviderForm } from "@/components/providers/provider-form"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogBody,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog"

export function ProviderDialog({ trigger, ...source }: ProviderFormSource & { trigger: ReactElement }) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const form = useProviderForm(source, () => setOpen(false))
  return (
    <Dialog open={open} onOpenChange={(value) => { setOpen(value); if (value) form.reset() }}>
      <DialogTrigger asChild>{trigger}</DialogTrigger>
      <DialogContent className="sm:max-w-2xl" showCloseButton={false}>
        <form className="flex min-h-0 flex-1 flex-col" onSubmit={form.submit}>
          <DialogHeader>
            <DialogTitle>{t(source.provider ? "providers.form.editTitle" : "providers.form.createTitle")}</DialogTitle>
            <DialogDescription>{t("providers.subtitle")}</DialogDescription>
          </DialogHeader>
          <DialogBody>{form.fields}{form.submitError}</DialogBody>
          <DialogFooter>
            <DialogClose asChild><Button type="button" variant="outline">{t("common.actions.cancel")}</Button></DialogClose>
            <Button type="submit" disabled={form.saving}>{t(form.saving ? "common.actions.saving" : source.provider ? "common.actions.save" : "common.actions.create")}</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

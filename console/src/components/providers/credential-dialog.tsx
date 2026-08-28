import type { ReactElement } from "react"
import type { ChannelDto } from "@/generated/ChannelDto"
import type { CredentialDto } from "@/generated/CredentialDto"
import type { CredentialWriteRequest } from "@/generated/CredentialWriteRequest"
import type { TlsPresetDto } from "@/generated/TlsPresetDto"
import { useState } from "react"
import { useTranslation } from "react-i18next"
import { CredentialForm } from "./credential-form"
import { CredentialWizard } from "./credential-wizard"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog"

export function CredentialDialog({ providerId, channel, credential, presets, trigger, onSave }: {
  providerId: number
  channel?: ChannelDto
  credential?: CredentialDto
  presets: Array<TlsPresetDto>
  trigger: ReactElement
  onSave: (value: CredentialWriteRequest, id?: number) => Promise<void>
}) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const [manual, setManual] = useState(false)
  const guided = !credential && channel?.login != null
  return (
    <Dialog open={open} onOpenChange={(value) => { setOpen(value); if (value) setManual(false) }}>
      <DialogTrigger asChild>{trigger}</DialogTrigger>
      <DialogContent className="sm:max-w-xl" showCloseButton={false}>
        <DialogHeader>
          <DialogTitle>{t(credential ? "common.actions.edit" : "providers.credentials.add")}</DialogTitle>
        </DialogHeader>
        {channel == null ? null : guided && !manual ? (
          <div className="min-h-0 overflow-x-hidden overflow-y-auto p-4">
            <CredentialWizard providerId={providerId} channel={channel} onDone={() => setOpen(false)} />
          </div>
        ) : channel == null ? null : (
          <CredentialForm providerId={providerId} channel={channel} credential={credential} presets={presets} onSave={onSave} onDone={() => setOpen(false)} />
        )}
        {guided && !manual ? (
          <DialogFooter><DialogClose asChild><Button type="button" variant="outline">{t("common.actions.cancel")}</Button></DialogClose><Button type="button" onClick={() => setManual(true)}>{t("providers.credentials.manual")}</Button></DialogFooter>
        ) : null}
      </DialogContent>
    </Dialog>
  )
}

import { PencilIcon } from "lucide-react"
import { useId } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import type { ChannelDto } from "@/generated/ChannelDto"
import type { CredentialDto } from "@/generated/CredentialDto"
import type { CredentialWriteRequest } from "@/generated/CredentialWriteRequest"
import type { TlsPresetDto } from "@/generated/TlsPresetDto"
import { CredentialDialog } from "@/components/providers/credential-dialog"
import { EntityDeleteButton } from "@/components/entity-delete-button"
import { Button } from "@/components/ui/button"
import { Field, FieldLabel } from "@/components/ui/field"
import { Switch } from "@/components/ui/switch"

export function CredentialRowActions({ credential, channel, presets, saving, onSave }: { credential: CredentialDto; channel?: ChannelDto; presets: Array<TlsPresetDto>; saving: boolean; onSave: (value: CredentialWriteRequest, id?: number) => Promise<void> }) {
  const { t } = useTranslation()
  const switchId = useId()
  const name = credential.label ?? t("providers.credentials.unnamed", { id: credential.id })
  const setEnabled = async (enabled: boolean) => {
    try {
      await onSave({
        provider_id: credential.provider_id,
        label: credential.label,
        kind: credential.kind,
        secret: null,
        enabled,
        weight: credential.weight,
        rpm_limit: credential.rpm_limit,
        tpm_limit: credential.tpm_limit,
        proxy_url: credential.proxy_url,
        tls_fingerprint: credential.tls_fingerprint,
      }, credential.id)
      toast.success(t("providers.credentials.updated"))
    } catch {
      toast.error(t("providers.credentials.updateError"))
    }
  }
  return (
    <div className="flex items-center justify-end gap-2" onClick={(event) => event.stopPropagation()}>
      <Field orientation="horizontal" className="w-auto">
        <FieldLabel htmlFor={switchId} className="sr-only">{t("providers.credentials.enabled")}</FieldLabel>
        <Switch id={switchId} size="sm" checked={credential.enabled} onCheckedChange={(value) => void setEnabled(value)} disabled={saving} />
      </Field>
      <CredentialDialog
        providerId={credential.provider_id}
        credential={credential}
        channel={channel}
        presets={presets}
        onSave={onSave}
        trigger={<Button variant="outline" size="icon-sm" aria-label={`${t("common.actions.edit")}: ${name}`}><PencilIcon aria-hidden /></Button>}
      />
      <EntityDeleteButton entity="credentials" id={credential.id} label={name} queryKeys={["credentials", "credential-cycles"]} />
    </div>
  )
}

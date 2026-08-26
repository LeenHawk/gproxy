import type { ChannelDto } from "@/generated/ChannelDto"
import type { CredentialDto } from "@/generated/CredentialDto"
import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import type { CredentialWriteRequest } from "@/generated/CredentialWriteRequest"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { ProviderWriteRequest } from "@/generated/ProviderWriteRequest"
import type { TlsPresetDto } from "@/generated/TlsPresetDto"
import { PencilIcon } from "lucide-react"
import { useId } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { CredentialList } from "@/components/providers/credential-list"
import { ProviderDialog } from "@/components/providers/provider-dialog"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Field, FieldLabel } from "@/components/ui/field"
import { Switch } from "@/components/ui/switch"

type Props = {
  provider: ProviderDto
  channels: Array<ChannelDto>
  channelsLoading: boolean
  channelsError: boolean
  presets: Array<TlsPresetDto>
  presetsLoading: boolean
  presetsError: boolean
  credentials: Array<CredentialDto>
  cyclesByCredential: Map<number, Array<CredentialQuotaCycleDto>>
  credentialsLoading: boolean
  credentialsError: boolean
  cyclesLoading: boolean
  cyclesError: boolean
  savingProviderId: number | null
  savingCredentialId: number | null
  onSaveProvider: (value: ProviderWriteRequest, id?: number) => Promise<void>
  onSaveCredential: (value: CredentialWriteRequest, id?: number) => Promise<void>
}

export function ProviderCard(props: Props) {
  const { t } = useTranslation()
  const id = useId()
  const provider = props.provider
  const invalidFingerprint = provider.invalid_tls_fingerprint != null || provider.tls_fingerprint_error != null
  const channel = props.channels.find((item) => item.id === provider.channel)

  const setEnabled = async (enabled: boolean) => {
    const value: ProviderWriteRequest = {
      name: provider.name,
      channel: provider.channel,
      settings: provider.settings,
      tls_fingerprint: provider.tls_fingerprint,
      enabled,
    }
    try {
      await props.onSaveProvider(value, provider.id)
      toast.success(t("providers.form.updated"))
    } catch {
      toast.error(t("providers.form.updateError"))
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{provider.name}</CardTitle>
        <CardDescription className="flex flex-wrap items-center gap-2">
          {channel ? <span>{channel.display_name}</span> : null}
          <span className="machine-text">{provider.channel}</span>
        </CardDescription>
        <CardAction className="flex flex-wrap items-center justify-end gap-2">
          <Badge variant={provider.enabled ? "outline" : "secondary"}>
            {t(`common.status.${provider.enabled ? "enabled" : "disabled"}`)}
          </Badge>
          <Field orientation="horizontal" className="w-auto">
            <FieldLabel htmlFor={`${id}-enabled`} className="sr-only">{t("providers.fields.enabled")}</FieldLabel>
            <Switch
              id={`${id}-enabled`}
              size="sm"
              checked={provider.enabled}
              onCheckedChange={(value) => void setEnabled(value)}
              disabled={props.savingProviderId === provider.id || invalidFingerprint}
            />
          </Field>
          <ProviderDialog
            provider={provider}
            channels={props.channels}
            channelsLoading={props.channelsLoading}
            channelsError={props.channelsError}
            presets={props.presets}
            presetsLoading={props.presetsLoading}
            presetsError={props.presetsError}
            onSave={props.onSaveProvider}
            trigger={<Button variant="outline" size="sm" aria-label={`${t("common.actions.edit")}: ${provider.name}`}><PencilIcon data-icon="inline-start" />{t("common.actions.edit")}</Button>}
          />
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-5">
        {invalidFingerprint ? (
          <Alert variant="destructive">
            <AlertTitle>{t("providers.fingerprint.title")}</AlertTitle>
            <AlertDescription>{provider.tls_fingerprint_error ?? t("providers.fingerprint.invalid")}</AlertDescription>
          </Alert>
        ) : null}
        <CredentialList
          providerId={provider.id}
          channel={channel}
          credentials={props.credentials}
          cyclesByCredential={props.cyclesByCredential}
          credentialsLoading={props.credentialsLoading}
          credentialsError={props.credentialsError}
          cyclesLoading={props.cyclesLoading}
          cyclesError={props.cyclesError}
          savingCredentialId={props.savingCredentialId}
          onSave={props.onSaveCredential}
        />
      </CardContent>
    </Card>
  )
}

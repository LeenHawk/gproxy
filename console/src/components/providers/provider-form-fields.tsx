import { useTranslation } from "react-i18next"
import { FingerprintField } from "@/components/providers/fingerprint-field"
import type { ProviderDraft, ProviderFormErrors, ProviderFormSource } from "@/components/providers/provider-form"
import { ProviderIdentityFields } from "@/components/providers/provider-identity-fields"
import { ProviderSettingsFields } from "@/components/providers/provider-settings-fields"
import { SearchableSelect } from "@/components/searchable-select"
import { Badge } from "@/components/ui/badge"
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"

export function ProviderFormFields(props: {
  id: string
  source: ProviderFormSource
  draft: ProviderDraft
  errors: ProviderFormErrors
  serverFingerprintError: string
  onChange: <K extends keyof ProviderDraft>(key: K, value: ProviderDraft[K]) => void
  onSelectPreset: (value: string) => void
  onCustomFingerprint: (value: string) => void
}) {
  const { t } = useTranslation()
  const { id, source, draft, errors } = props
  const selectedChannel = source.channels.find((channel) => channel.id === draft.channel)
  return (
    <FieldGroup>
      <Field data-invalid={Boolean(errors.name) || undefined}>
        <FieldLabel htmlFor={`${id}-name`}>{t("providers.fields.name")}</FieldLabel>
        <Input id={`${id}-name`} value={draft.name} onChange={(event) => props.onChange("name", event.target.value)} aria-invalid={Boolean(errors.name) || undefined} />
        {errors.name ? <FieldError>{errors.name}</FieldError> : null}
      </Field>
      <ProviderIdentityFields id={id} providerId={source.provider?.id} label={draft.label} strategy={draft.credentialStrategy} proxyUrl={draft.proxyUrl} onLabel={(value) => props.onChange("label", value)} onStrategy={(value) => props.onChange("credentialStrategy", value)} onProxy={(value) => props.onChange("proxyUrl", value)} />
      <Field data-invalid={Boolean(errors.channel) || source.channelsError || undefined}>
        <FieldLabel htmlFor={`${id}-channel`}>{t("providers.fields.channel")}</FieldLabel>
        <SearchableSelect
          value={draft.channel}
          id={`${id}-channel`}
          options={source.channels.map((channel) => ({ value: channel.id, label: channel.display_name, keywords: channel.id }))}
          placeholder={source.channelsLoading ? t("common.loading") : t("common.none")}
          searchPlaceholder={t("common.search")}
          emptyLabel={t("common.none")}
          ariaLabel={t("providers.fields.channel")}
          disabled={source.channelsLoading || source.provider !== undefined}
          onChange={(value) => props.onChange("channel", value)}
        />
        {source.provider ? null : <FieldDescription>{t("providers.form.channelHint")}</FieldDescription>}
        {selectedChannel ? <div className="flex flex-wrap items-center gap-2"><span className="text-xs text-muted-foreground">{t("providers.form.channelCapabilities", { count: selectedChannel.supports.length })}</span>{[...new Set(selectedChannel.supports.map((support) => support.group))].map((group) => <Badge key={group} variant="outline">{t(`rules.groups.${group}`, { defaultValue: group })}</Badge>)}</div> : null}
        {source.channelsError ? <FieldError>{t("common.errors.load")}</FieldError> : null}
        {errors.channel ? <FieldError>{errors.channel}</FieldError> : null}
      </Field>
      <ProviderSettingsFields
        channel={selectedChannel}
        text={draft.settings}
        error={errors.settings}
        onChange={(value) => props.onChange("settings", value)}
        advancedChildren={(
          <FingerprintField
            text={draft.fingerprint}
            preset={draft.preset}
            presets={source.presets}
            presetsLoading={source.presetsLoading}
            presetsError={source.presetsError}
            validationError={errors.fingerprint}
            serverError={props.serverFingerprintError}
            onPresetChange={props.onSelectPreset}
            onTextChange={props.onCustomFingerprint}
          />
        )}
      />
      <Field orientation="horizontal">
        <FieldLabel htmlFor={`${id}-enabled`}>{t("providers.fields.enabled")}</FieldLabel>
        <Switch id={`${id}-enabled`} checked={draft.enabled} onCheckedChange={(value) => props.onChange("enabled", value)} />
      </Field>
    </FieldGroup>
  )
}

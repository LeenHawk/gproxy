import type { ReactElement } from "react"
import type { ChannelDto } from "@/generated/ChannelDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { ProviderWriteRequest } from "@/generated/ProviderWriteRequest"
import type { TlsPresetDto } from "@/generated/TlsPresetDto"
import { useId, useState } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { ApiError } from "@/api/client"
import { CUSTOM_FINGERPRINT, DEFAULT_FINGERPRINT, parseFingerprint } from "@/components/providers/fingerprint"
import { FingerprintField } from "@/components/providers/fingerprint-field"
import { parseJsonObject, prettyJson } from "@/components/providers/json"
import { ProviderSettingsFields } from "@/components/providers/provider-settings-fields"
import { SearchableSelect } from "@/components/searchable-select"
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
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"

type Draft = {
  name: string
  channel: string
  settings: string
  fingerprint: string
  preset: string
  enabled: boolean
}

function initialDraft(provider: ProviderDto | undefined, presets: Array<TlsPresetDto>): Draft {
  const rawFingerprint = provider?.invalid_tls_fingerprint ?? provider?.tls_fingerprint ?? null
  const preset = rawFingerprint == null
    ? DEFAULT_FINGERPRINT
    : presets.find((item) => JSON.stringify(item.fingerprint) === JSON.stringify(rawFingerprint))?.id ?? CUSTOM_FINGERPRINT
  return {
    name: provider?.name ?? "",
    channel: provider?.channel ?? "",
    settings: prettyJson(provider?.settings ?? {}),
    fingerprint: prettyJson(rawFingerprint),
    preset,
    enabled: provider?.enabled ?? true,
  }
}

type Props = {
  provider?: ProviderDto
  channels: Array<ChannelDto>
  channelsLoading: boolean
  channelsError: boolean
  presets: Array<TlsPresetDto>
  presetsLoading: boolean
  presetsError: boolean
  trigger: ReactElement
  onSave: (value: ProviderWriteRequest, id?: number) => Promise<void>
}

export function ProviderDialog(props: Props) {
  const { t } = useTranslation()
  const id = useId()
  const [open, setOpen] = useState(false)
  const [draft, setDraft] = useState(() => initialDraft(props.provider, props.presets))
  const [errors, setErrors] = useState({ name: "", channel: "", settings: "", fingerprint: "", submit: "" })
  const [serverFingerprintError, setServerFingerprintError] = useState(props.provider?.tls_fingerprint_error ?? "")
  const [saving, setSaving] = useState(false)

  const reset = () => {
    setDraft(initialDraft(props.provider, props.presets))
    setErrors({ name: "", channel: "", settings: "", fingerprint: "", submit: "" })
    setServerFingerprintError(props.provider?.tls_fingerprint_error ?? "")
  }
  const change = <K extends keyof Draft>(key: K, value: Draft[K]) => setDraft((current) => ({ ...current, [key]: value }))
  const selectPreset = (value: string) => {
    change("preset", value)
    setServerFingerprintError("")
    setErrors((current) => ({ ...current, fingerprint: "" }))
    if (value === DEFAULT_FINGERPRINT) change("fingerprint", "")
    else if (value !== CUSTOM_FINGERPRINT) change("fingerprint", prettyJson(props.presets.find((item) => item.id === value)?.fingerprint))
  }
  const selectedChannel = props.channels.find((channel) => channel.id === draft.channel)

  const submit = async (event: React.FormEvent) => {
    event.preventDefault()
    const settings = parseJsonObject<Record<string, unknown>>(draft.settings)
    const fingerprint = parseFingerprint(draft.fingerprint)
    const nextErrors = {
      name: draft.name.trim() ? "" : t("common.errors.required"),
      channel: props.channels.some((channel) => channel.id === draft.channel) ? "" : t("common.errors.invalid"),
      settings: settings.ok ? "" : t("providers.form.settingsInvalid"),
      fingerprint: fingerprint.ok ? "" : t("providers.fingerprint.invalid"),
      submit: "",
    }
    setErrors(nextErrors)
    if (Object.values(nextErrors).some(Boolean) || !settings.ok || !fingerprint.ok) return
    const value: ProviderWriteRequest = {
      name: draft.name.trim(),
      channel: draft.channel,
      settings: settings.value,
      tls_fingerprint: fingerprint.value,
      enabled: draft.enabled,
    }
    setSaving(true)
    try {
      await props.onSave(value, props.provider?.id)
      toast.success(t(props.provider ? "providers.form.updated" : "providers.form.created"))
      setOpen(false)
    } catch (error) {
      const message = error instanceof ApiError && error.status === 400
        ? error.message
        : t(props.provider ? "providers.form.updateError" : "providers.form.createError")
      setErrors((current) => ({ ...current, submit: message }))
    } finally {
      setSaving(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={(value) => { setOpen(value); if (value) reset() }}>
      <DialogTrigger asChild>{props.trigger}</DialogTrigger>
      <DialogContent className="sm:max-w-2xl" showCloseButton={false}>
        <form className="flex min-h-0 flex-1 flex-col" onSubmit={submit}>
          <DialogHeader>
            <DialogTitle>{t(props.provider ? "providers.form.editTitle" : "providers.form.createTitle")}</DialogTitle>
            <DialogDescription>{t("providers.subtitle")}</DialogDescription>
          </DialogHeader>
          <DialogBody><FieldGroup>
            <Field data-invalid={Boolean(errors.name) || undefined}>
              <FieldLabel htmlFor={`${id}-name`}>{t("providers.fields.name")}</FieldLabel>
              <Input id={`${id}-name`} value={draft.name} onChange={(event) => change("name", event.target.value)} aria-invalid={Boolean(errors.name) || undefined} />
              {errors.name ? <FieldError>{errors.name}</FieldError> : null}
            </Field>
            <Field data-invalid={Boolean(errors.channel) || props.channelsError || undefined}>
              <FieldLabel htmlFor={`${id}-channel`}>{t("providers.fields.channel")}</FieldLabel>
              <SearchableSelect
                value={draft.channel}
                id={`${id}-channel`}
                options={props.channels.map((channel) => ({ value: channel.id, label: channel.display_name, keywords: channel.id }))}
                placeholder={props.channelsLoading ? t("common.loading") : t("common.none")}
                searchPlaceholder={t("common.search")}
                emptyLabel={t("common.none")}
                ariaLabel={t("providers.fields.channel")}
                disabled={props.channelsLoading}
                onChange={(value) => change("channel", value)}
              />
              <FieldDescription>{t("providers.form.channelHint")}</FieldDescription>
              {props.channelsError ? <FieldError>{t("common.errors.load")}</FieldError> : null}
              {errors.channel ? <FieldError>{errors.channel}</FieldError> : null}
            </Field>
            <ProviderSettingsFields
              channel={selectedChannel}
              text={draft.settings}
              error={errors.settings}
              onChange={(value) => change("settings", value)}
              advancedChildren={(
                <FingerprintField
                  text={draft.fingerprint}
                  preset={draft.preset}
                  presets={props.presets}
                  presetsLoading={props.presetsLoading}
                  presetsError={props.presetsError}
                  validationError={errors.fingerprint}
                  serverError={serverFingerprintError}
                  onPresetChange={selectPreset}
                  onTextChange={(value) => { change("fingerprint", value); change("preset", CUSTOM_FINGERPRINT); setServerFingerprintError("") }}
                />
              )}
            />
            <Field orientation="horizontal">
              <FieldLabel htmlFor={`${id}-enabled`}>{t("providers.fields.enabled")}</FieldLabel>
              <Switch id={`${id}-enabled`} checked={draft.enabled} onCheckedChange={(value) => change("enabled", value)} />
            </Field>
          </FieldGroup>
          {errors.submit ? <FieldError>{errors.submit}</FieldError> : null}</DialogBody>
          <DialogFooter>
            <DialogClose asChild><Button type="button" variant="outline">{t("common.actions.cancel")}</Button></DialogClose>
            <Button type="submit" disabled={saving}>{t(saving ? "common.actions.saving" : props.provider ? "common.actions.save" : "common.actions.create")}</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

import type { ReactNode } from "react"
import type { ChannelDto } from "@/generated/ChannelDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { ProviderWriteRequest } from "@/generated/ProviderWriteRequest"
import type { TlsPresetDto } from "@/generated/TlsPresetDto"
import { useId, useState } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { ApiError } from "@/api/client"
import { CUSTOM_FINGERPRINT, DEFAULT_FINGERPRINT, parseFingerprint } from "@/components/providers/fingerprint"
import { parseJsonObject, prettyJson } from "@/components/providers/json"
import { ProviderFormFields } from "@/components/providers/provider-form-fields"
import { FieldError } from "@/components/ui/field"

export type ProviderFormSource = {
  provider?: ProviderDto
  channels: Array<ChannelDto>
  channelsLoading: boolean
  channelsError: boolean
  presets: Array<TlsPresetDto>
  presetsLoading: boolean
  presetsError: boolean
  onSave: (value: ProviderWriteRequest, id?: number) => Promise<void>
}

export type ProviderDraft = {
  name: string
  label: string
  channel: string
  credentialStrategy: string
  proxyUrl: string
  settings: string
  fingerprint: string
  preset: string
  enabled: boolean
}

export type ProviderFormErrors = { name: string; channel: string; settings: string; fingerprint: string; submit: string }

const NO_ERRORS: ProviderFormErrors = { name: "", channel: "", settings: "", fingerprint: "", submit: "" }

function initialDraft(provider: ProviderDto | undefined, presets: Array<TlsPresetDto>): ProviderDraft {
  const rawFingerprint = provider?.invalid_tls_fingerprint ?? provider?.tls_fingerprint ?? null
  const preset = rawFingerprint == null
    ? DEFAULT_FINGERPRINT
    : presets.find((item) => JSON.stringify(item.fingerprint) === JSON.stringify(rawFingerprint))?.id ?? CUSTOM_FINGERPRINT
  return {
    name: provider?.name ?? "",
    label: provider?.label ?? "",
    channel: provider?.channel ?? "",
    credentialStrategy: provider?.credential_strategy ?? "round_robin",
    proxyUrl: provider?.proxy_url ?? "",
    settings: prettyJson(provider?.settings ?? {}),
    fingerprint: prettyJson(rawFingerprint),
    preset,
    enabled: provider?.enabled ?? true,
  }
}

export function useProviderForm(source: ProviderFormSource, onSaved?: () => void) {
  const { t } = useTranslation()
  const id = useId()
  const [draft, setDraft] = useState(() => initialDraft(source.provider, source.presets))
  const [errors, setErrors] = useState(NO_ERRORS)
  const [serverFingerprintError, setServerFingerprintError] = useState(source.provider?.tls_fingerprint_error ?? "")
  const [saving, setSaving] = useState(false)

  const reset = () => {
    setDraft(initialDraft(source.provider, source.presets))
    setErrors(NO_ERRORS)
    setServerFingerprintError(source.provider?.tls_fingerprint_error ?? "")
  }
  const change = <K extends keyof ProviderDraft>(key: K, value: ProviderDraft[K]) => setDraft((current) => ({ ...current, [key]: value }))
  const selectPreset = (value: string) => {
    change("preset", value)
    setServerFingerprintError("")
    setErrors((current) => ({ ...current, fingerprint: "" }))
    if (value === DEFAULT_FINGERPRINT) change("fingerprint", "")
    else if (value !== CUSTOM_FINGERPRINT) change("fingerprint", prettyJson(source.presets.find((item) => item.id === value)?.fingerprint))
  }

  const submit = async (event: React.FormEvent) => {
    event.preventDefault()
    const settings = parseJsonObject<Record<string, unknown>>(draft.settings)
    const fingerprint = parseFingerprint(draft.fingerprint)
    const nextErrors: ProviderFormErrors = {
      name: draft.name.trim() ? "" : t("common.errors.required"),
      channel: source.channels.some((channel) => channel.id === draft.channel) ? "" : t("common.errors.invalid"),
      settings: settings.ok ? "" : t("providers.form.settingsInvalid"),
      fingerprint: fingerprint.ok ? "" : t("providers.fingerprint.invalid"),
      submit: "",
    }
    setErrors(nextErrors)
    if (Object.values(nextErrors).some(Boolean) || !settings.ok || !fingerprint.ok) return
    const value: ProviderWriteRequest = {
      name: draft.name.trim(),
      label: draft.label.trim() || null,
      channel: draft.channel,
      settings: settings.value,
      credential_strategy: draft.credentialStrategy,
      proxy_url: draft.proxyUrl.trim() || null,
      tls_fingerprint: fingerprint.value,
      enabled: draft.enabled,
    }
    setSaving(true)
    try {
      await source.onSave(value, source.provider?.id)
      toast.success(t(source.provider ? "providers.form.updated" : "providers.form.created"))
      onSaved?.()
    } catch (error) {
      const message = error instanceof ApiError && error.status === 400
        ? error.message
        : t(source.provider ? "providers.form.updateError" : "providers.form.createError")
      setErrors((current) => ({ ...current, submit: message }))
    } finally {
      setSaving(false)
    }
  }

  const fields = (
    <ProviderFormFields
      id={id}
      source={source}
      draft={draft}
      errors={errors}
      serverFingerprintError={serverFingerprintError}
      onChange={change}
      onSelectPreset={selectPreset}
      onCustomFingerprint={(value) => { change("fingerprint", value); change("preset", CUSTOM_FINGERPRINT); setServerFingerprintError("") }}
    />
  )
  const submitError: ReactNode = errors.submit ? <FieldError>{errors.submit}</FieldError> : null
  return { fields, submitError, submit, saving, reset }
}

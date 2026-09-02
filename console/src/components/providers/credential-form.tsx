import type { FormEvent } from "react"
import type { ChannelDto } from "@/generated/ChannelDto"
import type { CredentialDto } from "@/generated/CredentialDto"
import type { CredentialWriteRequest } from "@/generated/CredentialWriteRequest"
import type { TlsPresetDto } from "@/generated/TlsPresetDto"
import { ChevronDownIcon } from "lucide-react"
import { useEffect, useMemo, useRef, useState } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { revealCredentialSecret } from "@/api/control"
import { ConnectivityTest } from "@/components/connectivity-test"
import { proxyProbe } from "@/lib/connectivity-probe"
import { CUSTOM_FINGERPRINT, DEFAULT_FINGERPRINT, parseFingerprint } from "./fingerprint"
import { FingerprintField } from "./fingerprint-field"
import { buildSecret, defaultCredentialKind, fieldsForCredentialKind, isSingleKey } from "@/components/providers/credential-secret"
import { CredentialSecretField } from "@/components/providers/credential-secret-field"
import { prettyJson } from "./json"
import { Button } from "@/components/ui/button"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { DialogClose, DialogFooter } from "@/components/ui/dialog"
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/components/ui/input-group"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"

function initialFingerprint(credential: CredentialDto | undefined, presets: Array<TlsPresetDto>) {
  const raw = credential?.invalid_tls_fingerprint ?? credential?.tls_fingerprint ?? null
  const preset = raw == null
    ? DEFAULT_FINGERPRINT
    : presets.find((item) => JSON.stringify(item.fingerprint) === JSON.stringify(raw))?.id ?? CUSTOM_FINGERPRINT
  return { text: prettyJson(raw), preset }
}

export function CredentialForm({ providerId, channel, credential, presets, onSave, onDone }: {
  providerId: number
  channel: ChannelDto
  credential?: CredentialDto
  presets: Array<TlsPresetDto>
  onSave: (value: CredentialWriteRequest, id?: number) => Promise<void>
  onDone: () => void
}) {
  const { t } = useTranslation()
  const [label, setLabel] = useState(credential?.label ?? "")
  const [kind, setKind] = useState(credential?.kind ?? defaultCredentialKind(channel.credential_fields))
  const [secretText, setSecretText] = useState("")
  const [weight, setWeight] = useState(String(credential?.weight ?? 100))
  const [rpm, setRpm] = useState(credential?.rpm_limit?.toString() ?? "")
  const [tpm, setTpm] = useState(credential?.tpm_limit?.toString() ?? "")
  const [proxyUrl, setProxyUrl] = useState(credential?.proxy_url ?? "")
  const [fingerprint, setFingerprint] = useState(() => initialFingerprint(credential, presets))
  const [enabled, setEnabled] = useState(credential?.enabled ?? true)
  const [error, setError] = useState("")
  const [saving, setSaving] = useState(false)
  const fields = useMemo(() => fieldsForCredentialKind(
    credential
      ? channel.credential_fields.map((field) => ({ ...field, required: false }))
      : channel.credential_fields,
    kind,
  ), [channel.credential_fields, credential, kind])

  /* v2 parity: editing prefills the stored secret so the operator sees what
     the credential holds. A failed reveal leaves the box empty, where blank
     still means "keep the stored value". */
  const prefilled = useRef(false)
  useEffect(() => {
    if (!credential || prefilled.current) return
    prefilled.current = true
    revealCredentialSecret(credential.id)
      .then((result) => {
        const secret = result.secret as Record<string, unknown> | null
        const single = isSingleKey(fields) ? secret?.[fields[0].key] : undefined
        const text = typeof single === "string" ? single : JSON.stringify(secret, null, 2)
        setSecretText((current) => (current === "" ? text : current))
      })
      .catch(() => {})
  }, [credential, fields])

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    const tls = parseFingerprint(fingerprint.text)
    const secretValue = buildSecret(fields, secretText)
    if (!credential && secretValue === null) {
      setError(t("providers.credentials.secretRequired"))
      return
    }
    if (credential && secretText.trim() !== "" && secretValue === null) {
      setError(t("providers.credentials.secretInvalid"))
      return
    }
    if (!tls.ok) {
      setError(t("providers.fingerprint.invalid"))
      return
    }
    setSaving(true)
    setError("")
    try {
      await onSave({
        provider_id: providerId,
        label: label.trim() || null,
        kind,
        secret: secretValue,
        enabled,
        weight: Number(weight),
        rpm_limit: rpm.trim() ? Number(rpm) : null,
        tpm_limit: tpm.trim() ? Number(tpm) : null,
        proxy_url: proxyUrl.trim() || null,
        tls_fingerprint: tls.value,
      }, credential?.id)
      toast.success(t(credential ? "providers.credentials.updated" : "providers.credentials.created"))
      onDone()
    } catch {
      setError(t(credential ? "providers.credentials.updateError" : "providers.credentials.createError"))
    } finally {
      setSaving(false)
    }
  }

  return (
    <form className="flex min-h-0 flex-1 flex-col" onSubmit={(event) => void submit(event)}>
      <FieldGroup className="min-h-0 overflow-x-hidden overflow-y-auto p-4">
        <Field>
          <FieldLabel htmlFor="credential-label">{t("providers.credentials.label")}</FieldLabel>
          <Input id="credential-label" value={label} onChange={(event) => setLabel(event.target.value)} />
        </Field>
        <Field>
          <FieldLabel htmlFor="credential-kind">{t("providers.credentials.kind")}</FieldLabel>
          <Select value={kind} onValueChange={setKind}>
            <SelectTrigger id="credential-kind" className="w-full"><SelectValue /></SelectTrigger>
            <SelectContent>
              {(["api_key", "oauth", "cookie"] as const).map((value) => <SelectItem key={value} value={value}>{t(`providers.credentials.kinds.${value}`)}</SelectItem>)}
            </SelectContent>
          </Select>
          <FieldDescription>{t("providers.credentials.kindHint")}</FieldDescription>
        </Field>
        <CredentialSecretField fields={fields} value={secretText} onChange={setSecretText} editing={credential !== undefined} />
        <Field>
          <FieldLabel htmlFor="credential-weight">{t("providers.credentials.weight")}</FieldLabel>
          <Input id="credential-weight" type="number" min={1} step={1} required value={weight} onChange={(event) => setWeight(event.target.value)} />
        </Field>
        <Collapsible data-field-span="full">
          <CollapsibleTrigger asChild>
            <Button type="button" variant="outline" className="group w-full justify-between">
              <span>{t("providers.form.advanced")}</span>
              <ChevronDownIcon className="transition-transform group-data-[state=open]:rotate-180" />
            </Button>
          </CollapsibleTrigger>
          <CollapsibleContent className="flex flex-col gap-4 pt-4">
            <div className="grid gap-4 sm:grid-cols-2">
              <Field><FieldLabel htmlFor="credential-rpm">{t("providers.credentials.rpm")}</FieldLabel><Input id="credential-rpm" type="number" min={1} step={1} value={rpm} onChange={(event) => setRpm(event.target.value)} /></Field>
              <Field><FieldLabel htmlFor="credential-tpm">{t("providers.credentials.tpm")}</FieldLabel><Input id="credential-tpm" type="number" min={1} step={1} value={tpm} onChange={(event) => setTpm(event.target.value)} /></Field>
            </div>
            <Field><FieldLabel htmlFor="credential-proxy">{t("providers.credentials.proxy")}</FieldLabel><InputGroup><InputGroupInput id="credential-proxy" type="url" className="font-mono" value={proxyUrl} onChange={(event) => setProxyUrl(event.target.value)} /><InputGroupAddon align="inline-end"><ConnectivityTest request={proxyProbe(proxyUrl, { provider_id: providerId, credential_id: credential?.id ?? null })} label={t("providers.credentials.proxy")} /></InputGroupAddon></InputGroup></Field>
            <FingerprintField
              text={fingerprint.text}
              preset={fingerprint.preset}
              presets={presets}
              presetsLoading={false}
              presetsError={false}
              validationError=""
              serverError={credential?.tls_fingerprint_error ?? ""}
              onPresetChange={(preset) => {
                const value = preset === DEFAULT_FINGERPRINT ? null : presets.find((item) => item.id === preset)?.fingerprint ?? null
                setFingerprint({ preset, text: prettyJson(value) })
              }}
              onTextChange={(text) => setFingerprint({ preset: CUSTOM_FINGERPRINT, text })}
            />
          </CollapsibleContent>
        </Collapsible>
        <Field orientation="horizontal"><FieldLabel htmlFor="credential-enabled">{t("providers.credentials.enabled")}</FieldLabel><Switch id="credential-enabled" checked={enabled} onCheckedChange={setEnabled} /></Field>
        {error ? <FieldError>{error}</FieldError> : null}
      </FieldGroup>
      <DialogFooter>
        <DialogClose asChild><Button type="button" variant="outline">{t("common.actions.cancel")}</Button></DialogClose>
        <Button type="submit" disabled={saving}>{t(saving ? "common.actions.saving" : "common.actions.save")}</Button>
      </DialogFooter>
    </form>
  )
}

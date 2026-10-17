import type { ReactNode } from "react"
import { useTranslation } from "react-i18next"
import { optionSource } from "@/api/options"
import { connectionProfiles } from "@/api/configuration"
import type { CredentialDto } from "@/generated/sdk"
import { useConsoleContext } from "@/capability/session"
import type { CollectionProps } from "@/pages/identity/collection"
import { credentialFields } from "@/pages/providers/fields"
import { RecordDialog, RecordEditor, type FormField } from "@/components/record-form"
import { ErrorNotice } from "@/components/state"
import { ownerColumns, ownerValue, useOwnerChoices } from "./ownership"

type FormProps = Parameters<NonNullable<CollectionProps<CredentialDto, unknown, unknown>["renderForm"]>>[0] & { providerId: string; channel: string; credentialKinds: string[]; paidUsage: boolean; inline?: boolean; secretActions?: ReactNode }
export function CredentialForm({ providerId, channel, credentialKinds, paidUsage, inline = false, secretActions, ...props }: FormProps) {
  const { t } = useTranslation()
  const context = useConsoleContext()
  const ownership = useOwnerChoices()
  const original = props.original
  const fixedKind = credentialKinds.length === 1 ? credentialKinds[0] : undefined
  const choicesForKinds = credentialKinds.map(value => ({ value, label: t(`credentialKinds.${value}`, { defaultValue: value }) }))
  const supportsRegion = channel === "vertex" || channel === "vertexexpress"
  const supportsPaidUsage = paidUsage
  const metadata = (original?.metadata ?? {}) as Record<string, unknown>
  const allowPaidUsage = metadata.allow_paid_usage === true
  const formMetadata = { ...metadata }
  if (supportsPaidUsage) delete formMetadata.allow_paid_usage
  if (supportsRegion) delete formMetadata.location
  const selectedOwner = original ? ownerValue(original) : ownership.defaultOwner
  const choices = ownership.choices.some(row => row.value === selectedOwner) ? ownership.choices : [...ownership.choices, { value: selectedOwner, label: selectedOwner }]
  const fields: FormField[] = credentialFields.filter(field => context.scope?.kind === "instance" || !["connectionProfileId", "proxy"].includes(field.name)).map(field => field.name === "secret" ? { ...field, description: t("form.credentialSecretHint") } : field.name === "connectionProfileId" ? { ...field, kind: "searchable" as const, emptyLabel: t("form.unset"), source: context.has("configuration.connection-profiles") ? optionSource(connectionProfiles, row => ({ value: row.id, label: row.name })) : undefined } : field.name === "proxy" ? { ...field, proxyScope: () => original ? { scope: "credential" as const, credential_id: original.id } : { scope: "provider" as const, provider_id: providerId, parent: true } } : { ...field })
  const authField = fields.find(field => field.name === "authKind")
  if (authField) authField.choices = choicesForKinds
  if (fixedKind) fields.splice(fields.findIndex(field => field.name === "authKind"), 1)
  const secretField = fields.find(field => field.name === "secret")!
  const parseSecret = secretField.parse!
  secretField.parse = (text, values) => parseSecret(text, { ...values, authKind: String(fixedKind ?? original?.authKind ?? values.authKind) })
  if (channel === "vertex") secretField.description = t("form.vertexSecretHint")
  if (channel === "aws_bedrock") secretField.description = t("form.awsSecretHint")
  if (supportsPaidUsage) fields.splice(1, 0, {
    name: "allowPaidUsage", kind: "switch", defaultChecked: false,
    label: t("management.allowPaidUsage"), description: t(channel === "claudecode" ? "management.claudePaidUsageHint" : channel === "codex" ? "management.codexPaidUsageHint" : "management.paidUsageHint"),
  })
  if (supportsRegion) fields.splice(1, 0, {
    name: "location", kind: "text", nullable: true,
    label: t("providerConfig.region"), description: t("management.credentialRegionHint"),
  })
  const Form = inline ? RecordEditor : RecordDialog
  return <>{ownership.error ? <ErrorNotice error={ownership.error} /> : null}<Form {...props} extra={secretActions} extraAfter="secret" submitDisabled={!original && !credentialKinds.length} mode={original ? "edit" : "create"} title={t(original ? "edit.credentials" : "create.credentials")} original={{ ...original, expiresAtMs: channel === "vertex" && original?.expiresAtMs === 0 ? null : original?.expiresAtMs, authKind: original?.authKind ?? credentialKinds[0], metadata: formMetadata, location: metadata.location ?? null, allowPaidUsage, owner: selectedOwner }} fields={[...fields, { name: "owner", label: t("management.owner"), kind: "select", required: true, choices }]}
    onSubmit={body => {
      const { owner, location, allowPaidUsage: paid, ...rest } = body
      const editedMetadata = rest.metadata ?? metadata
      if (supportsPaidUsage && (paid !== undefined || rest.metadata !== undefined) && typeof editedMetadata === "object" && !Array.isArray(editedMetadata)) {
        rest.metadata = { ...(editedMetadata as Record<string, unknown>), allow_paid_usage: paid ?? allowPaidUsage }
      }
      if (supportsRegion && (location !== undefined || rest.metadata !== undefined)) {
        const updated = { ...(rest.metadata ?? metadata) as Record<string, unknown> }
        const region = String(location !== undefined ? location ?? "" : metadata.location ?? "").trim()
        if (region) updated.location = region
        else delete updated.location
        rest.metadata = updated
      }
      props.onSubmit({ ...rest, ...(!original && fixedKind ? { authKind: fixedKind } : {}), ...(owner !== undefined || !original ? ownerColumns(String(owner ?? selectedOwner)) : {}) })
    }}
  /></>
}

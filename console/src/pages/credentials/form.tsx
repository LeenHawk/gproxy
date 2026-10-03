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

type FormProps = Parameters<NonNullable<CollectionProps<CredentialDto, unknown, unknown>["renderForm"]>>[0] & { providerId: string; channel: string; inline?: boolean; secretActions?: ReactNode }
const paidUsageChannels = new Set(["codex", "claudecode", "cline", "antigravity", "devin", "kiro", "copilotcli", "grokbuild"])
export function CredentialForm({ providerId, channel, inline = false, secretActions, ...props }: FormProps) {
  const { t } = useTranslation()
  const context = useConsoleContext()
  const ownership = useOwnerChoices()
  const original = props.original
  const supportsPaidUsage = paidUsageChannels.has(channel)
  const metadata = (original?.metadata ?? {}) as Record<string, unknown>
  const allowPaidUsage = metadata.allow_paid_usage === true
  const formMetadata = { ...metadata }
  if (supportsPaidUsage) delete formMetadata.allow_paid_usage
  const selectedOwner = original ? ownerValue(original) : ownership.defaultOwner
  const choices = ownership.choices.some(row => row.value === selectedOwner) ? ownership.choices : [...ownership.choices, { value: selectedOwner, label: selectedOwner }]
  const fields: FormField[] = credentialFields.filter(field => context.scope?.kind === "instance" || !["connectionProfileId", "proxy"].includes(field.name)).map(field => field.name === "secret" ? { ...field, description: t("form.credentialSecretHint") } : field.name === "connectionProfileId" ? { ...field, kind: "searchable" as const, emptyLabel: t("form.unset"), source: context.has("configuration.connection-profiles") ? optionSource(connectionProfiles, row => ({ value: row.id, label: row.name })) : undefined } : field.name === "proxy" ? { ...field, proxyScope: () => original ? { scope: "credential" as const, credential_id: original.id } : { scope: "provider" as const, provider_id: providerId, parent: true } } : { ...field })
  if (channel === "devin") {
    const authField = fields.find(field => field.name === "authKind")
    if (authField) authField.choices = authField.choices?.filter(choice => choice.value === "oauth")
  }
  if (supportsPaidUsage) fields.splice(1, 0, {
    name: "allowPaidUsage", kind: "switch", defaultChecked: false,
    label: t("management.allowPaidUsage"), description: t(channel === "claudecode" ? "management.claudePaidUsageHint" : channel === "codex" ? "management.codexPaidUsageHint" : "management.paidUsageHint"),
  })
  const Form = inline ? RecordEditor : RecordDialog
  return <>{ownership.error ? <ErrorNotice error={ownership.error} /> : null}<Form {...props} extra={secretActions} extraAfter="secret" mode={original ? "edit" : "create"} title={t(original ? "edit.credentials" : "create.credentials")} original={{ ...original, metadata: formMetadata, allowPaidUsage, owner: selectedOwner }} fields={[...fields, { name: "owner", label: t("management.owner"), kind: "select", required: true, choices }]}
    onSubmit={body => {
      const { owner, allowPaidUsage: paid, ...rest } = body
      const editedMetadata = rest.metadata ?? metadata
      if (supportsPaidUsage && (paid !== undefined || rest.metadata !== undefined) && typeof editedMetadata === "object" && !Array.isArray(editedMetadata)) {
        rest.metadata = { ...(editedMetadata as Record<string, unknown>), allow_paid_usage: paid ?? allowPaidUsage }
      }
      props.onSubmit({ ...rest, ...(owner !== undefined || !original ? ownerColumns(String(owner ?? selectedOwner)) : {}) })
    }}
  /></>
}

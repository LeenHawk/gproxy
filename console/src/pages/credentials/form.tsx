import type { ReactNode } from "react"
import { useTranslation } from "react-i18next"
import { optionSource } from "@/api/options"
import { connectionProfiles } from "@/api/configuration"
import type { CredentialDto } from "@/generated/sdk"
import { useConsoleContext } from "@/capability/session"
import type { CollectionProps } from "@/pages/identity/collection"
import { credentialFields } from "@/pages/providers/fields"
import { RecordDialog, RecordEditor } from "@/components/record-form"
import { ErrorNotice } from "@/components/state"
import { ownerColumns, ownerValue, useOwnerChoices } from "./ownership"

type FormProps = Parameters<NonNullable<CollectionProps<CredentialDto, unknown, unknown>["renderForm"]>>[0] & { providerId: string; inline?: boolean; secretActions?: ReactNode }
export function CredentialForm({ providerId, inline = false, secretActions, ...props }: FormProps) {
  const { t } = useTranslation()
  const context = useConsoleContext()
  const ownership = useOwnerChoices()
  const original = props.original
  const selectedOwner = original ? ownerValue(original) : ownership.defaultOwner
  const choices = ownership.choices.some(row => row.value === selectedOwner) ? ownership.choices : [...ownership.choices, { value: selectedOwner, label: selectedOwner }]
  const fields = credentialFields.filter(field => context.scope?.kind === "instance" || !["connectionProfileId", "proxy"].includes(field.name)).map(field => field.name === "connectionProfileId" ? { ...field, kind: "searchable" as const, emptyLabel: t("form.unset"), source: context.has("configuration.connection-profiles") ? optionSource(connectionProfiles, row => ({ value: row.id, label: row.name })) : undefined } : field.name === "proxy" ? { ...field, proxyScope: () => original ? { scope: "credential" as const, credential_id: original.id } : { scope: "provider" as const, provider_id: providerId, parent: true } } : field)
  const Form = inline ? RecordEditor : RecordDialog
  return <>{ownership.error ? <ErrorNotice error={ownership.error} /> : null}<Form {...props} extra={secretActions} extraAfter="secret" mode={original ? "edit" : "create"} title={t(original ? "edit.credentials" : "create.credentials")} original={{ ...original, owner: selectedOwner }} fields={[...fields, { name: "owner", label: t("management.owner"), kind: "select", required: true, choices }]}
    onSubmit={body => { const { owner, ...rest } = body; props.onSubmit({ ...rest, ...(owner !== undefined || !original ? ownerColumns(String(owner ?? selectedOwner)) : {}) }) }}
  /></>
}

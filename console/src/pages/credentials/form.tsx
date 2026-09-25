import type { ReactNode } from "react"
import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
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
  const profiles = useQuery({ queryKey: ["admin", "/connection-profiles", "directory"], queryFn: connectionProfiles, enabled: context.has("configuration.connection-profiles") })
  const original = props.original
  const selectedOwner = original ? ownerValue(original) : ownership.defaultOwner
  const choices = ownership.choices.some(row => row.value === selectedOwner) ? ownership.choices : [...ownership.choices, { value: selectedOwner, label: selectedOwner }]
  const fields = credentialFields.filter(field => context.scope?.kind === "instance" || !["connectionProfileId", "proxy"].includes(field.name)).map(field => field.name === "connectionProfileId" ? { ...field, kind: "select" as const, choices: (profiles.data ?? []).map(row => ({ value: row.id, label: row.name })) } : field.name === "proxy" ? { ...field, proxyScope: () => original ? { scope: "credential" as const, credential_id: original.id } : { scope: "provider" as const, provider_id: providerId, parent: true } } : field)
  const Form = inline ? RecordEditor : RecordDialog
  return <>{ownership.error || profiles.error ? <ErrorNotice error={ownership.error ?? profiles.error} /> : null}<Form {...props} extra={secretActions} extraAfter="secret" mode={original ? "edit" : "create"} title={t(original ? "edit.credentials" : "create.credentials")} original={{ ...original, owner: selectedOwner }} fields={[...fields, { name: "owner", label: t("management.owner"), kind: "select", required: true, choices }]}
    onSubmit={body => { const { owner, ...rest } = body; props.onSubmit({ ...rest, ...(owner !== undefined || !original ? ownerColumns(String(owner ?? selectedOwner)) : {}) }) }}
  /></>
}

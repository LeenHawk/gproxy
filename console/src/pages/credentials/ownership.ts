import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { useConsoleContext } from "@/capability/session"
import { api } from "@/api/client"
import type { CredentialOwnerOptionDto } from "@/generated/app"
import type { CredentialOwner } from "@/generated/sdk"

export function ownerValue(owner: CredentialOwner) {
  return owner.userId ? `user:${owner.userId}` : owner.teamId ? `team:${owner.teamId}` : owner.organizationId ? `org:${owner.organizationId}` : "shared"
}
export function ownerColumns(value: string): CredentialOwner {
  const [kind, ...parts] = value.split(":")
  const id = parts.join(":")
  return { userId: kind === "user" ? id : null, teamId: kind === "team" ? id : null, organizationId: kind === "org" ? id : null }
}
export function useOwnerChoices() {
  const { t } = useTranslation()
  const context = useConsoleContext()
  const scope = context.scope
  const owners = useQuery({ queryKey: ["credential-owners"], queryFn: () => api<CredentialOwnerOptionDto[]>("/admin/api/credentials/owners") })
  const entries = [
    ...(scope?.kind === "instance" ? [{ value: "shared", label: t("management.shared") }] : []),
    ...(owners.data ?? []).map(row => ({ value: `${row.kind}:${row.id}`, label: `${t(`values.${row.kind}`)}: ${row.name}` })),
  ]
  const defaultOwner = scope?.kind === "organization" ? `org:${scope.id}` : scope?.kind === "team" ? `team:${scope.id}` : "shared"
  if (!entries.some(entry => entry.value === defaultOwner)) entries.push({ value: defaultOwner, label: scope?.name ?? defaultOwner })
  return { choices: entries, defaultOwner, error: owners.error }
}

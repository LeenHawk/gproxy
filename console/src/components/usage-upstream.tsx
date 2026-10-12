import { useQuery } from "@tanstack/react-query"
import { providers, credentials } from "@/api/configuration"
import { useConsoleContext } from "@/capability/session"
import { IdCell } from "@/components/data-table"

export function UsageProviderCell({ id }: { id: string | null }) {
  const context = useConsoleContext()
  const provider = useQuery({
    queryKey: ["admin", "/providers", "detail", id],
    queryFn: () => providers.get(id!),
    enabled: !!id && context.has("configuration.providers"),
    retry: false,
  })
  if (!provider.data) return <IdCell value={id ?? "—"} />
  return <span title={id ?? undefined}>{provider.data.displayName ?? provider.data.name}</span>
}

export function UsageCredentialCell({ id }: { id: string | null }) {
  const context = useConsoleContext()
  const credential = useQuery({
    queryKey: ["admin", "/credentials", id],
    queryFn: () => credentials.get(id!),
    enabled: !!id && context.has("configuration.credentials"),
    retry: false,
  })
  return credential.data?.label ? <span title={id ?? undefined}>{credential.data.label}</span> : <IdCell value={id ?? "—"} />
}

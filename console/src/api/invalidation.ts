import type { QueryClient } from "@tanstack/react-query"
/** Refresh dependent views, never the queries that perform upstream probes. */
export async function invalidateConfiguration(client: QueryClient, path: string) {
  const keys: readonly unknown[][] = path === "/quotas" ? [["quota-status"], ["credential-limits"], ["portal", "quota"]]
    : path === "/providers" ? [["credential-providers"]]
    : []
  await Promise.all([["admin", path], ...keys].map(queryKey => client.invalidateQueries({ queryKey })))
}

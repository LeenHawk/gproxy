import type { QueryClient } from "@tanstack/react-query"
/** Refresh dependent views, never the queries that perform upstream probes. */
export async function invalidateConfiguration(client: QueryClient, path: string) {
  const keys: readonly unknown[][] = path === "/quotas" ? [["quota-status"], ["credential-limits"], ["portal", "quota"]]
    : path === "/providers" ? [["credential-providers"], ["admin", "/rule-sets"], ["admin", "/provider-rule-sets"]]
    : []
  await Promise.all([["admin", path], ...( ["/models", "/provider-models", "/providers", "/price-rules"].includes(path) ? [["model-catalog"]] : []), ...keys].map(queryKey => client.invalidateQueries({ queryKey })))
}

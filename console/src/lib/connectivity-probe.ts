import type { ConnectivityTestRequest } from "@/generated/ConnectivityTestRequest"

// An empty proxy box is a question worth asking — "what does this egress through today?" —
// so it probes the enclosing entity rather than refusing to run. The answer's `proxy_source`
// names whichever override actually applied.
export function proxyProbe(proxyUrl: string, owner: { provider_id: number | null; credential_id: number | null }): ConnectivityTestRequest {
  const trimmed = proxyUrl.trim()
  if (trimmed) return { scope: "proxy", provider_id: null, credential_id: null, proxy_url: trimmed }
  if (owner.credential_id != null) return { scope: "credential", provider_id: null, credential_id: owner.credential_id, proxy_url: null }
  if (owner.provider_id != null) return { scope: "provider", provider_id: owner.provider_id, credential_id: null, proxy_url: null }
  return { scope: "global", provider_id: null, credential_id: null, proxy_url: null }
}

import type { CredentialDto } from "@/generated/CredentialDto"
import type { QuotaProbeResponse } from "@/generated/QuotaProbeResponse"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { credentialQuota, probeCredentialQuota } from "@/api/control"

const freshnessMs = 10 * 60 * 1000

export function useCredentialQuota(credential: CredentialDto) {
  const client = useQueryClient()
  const snapshotKey = ["credential-quota", credential.id, credential.version]
  const probeKey = ["credential-quota-probe", credential.id, credential.version]
  const saved = useQuery({
    queryKey: snapshotKey,
    queryFn: () => credentialQuota(credential.id),
    retry: false,
    staleTime: 30_000,
  })
  const sources = saved.data?.sources ?? []
  const canProbe = sources.some(({ capability }) => capability.mode === "probe" && capability.support === "ready")
  const storeResult = (result: QuotaProbeResponse) => {
    client.setQueryData(snapshotKey, result.snapshot)
    void client.invalidateQueries({ queryKey: ["credential-cycles"] })
  }
  const probe = useQuery({
    queryKey: probeKey,
    queryFn: async () => {
      const result = await probeCredentialQuota(credential.id)
      storeResult(result)
      return result
    },
    enabled: () => saved.isSuccess && sources.some(({ capability, attempted_at_ms, observed_at_ms }) =>
      capability.mode === "probe" && capability.support === "ready" && capability.automatic
      && Date.now() - Math.max(attempted_at_ms ?? 0, observed_at_ms ?? 0) >= freshnessMs,
    ),
    retry: false,
    staleTime: freshnessMs,
    gcTime: Infinity,
  })
  const manual = useMutation({
    mutationFn: () => probeCredentialQuota(credential.id, true),
    onSuccess: (result) => {
      storeResult(result)
      client.setQueryData(probeKey, result)
    },
  })
  return {
    snapshot: saved.data,
    quota: probe.data,
    canProbe,
    loading: saved.isPending,
    refreshing: probe.isFetching || manual.isPending,
    error: saved.error ?? manual.error ?? probe.error,
    refresh: manual.mutateAsync,
  }
}

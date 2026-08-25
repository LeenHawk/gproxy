import type { CredentialWriteRequest } from "@/generated/CredentialWriteRequest"
import type { ProviderWriteRequest } from "@/generated/ProviderWriteRequest"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
  credentials as fetchCredentials,
  providers as fetchProviders,
  saveCredential,
  saveProvider,
} from "@/api/control"
import {
  channels as fetchChannels,
  credentialCycles as fetchCredentialCycles,
  tlsPresets as fetchTlsPresets,
} from "@/api/observability"
import { ProvidersView } from "@/components/providers/providers-view"
import { useNow } from "@/lib/use-now"

const MAX_CYCLE_RANGE_SECONDS = 366 * 24 * 60 * 60

type ProviderMutation = { value: ProviderWriteRequest; id?: number }
type CredentialMutation = { value: CredentialWriteRequest; id?: number }

export function ProvidersPage() {
  const queryClient = useQueryClient()
  const to = useNow() + 1
  const cycleRange = { from: to - MAX_CYCLE_RANGE_SECONDS, to }
  const providers = useQuery({ queryKey: ["providers"], queryFn: fetchProviders })
  const credentials = useQuery({ queryKey: ["credentials"], queryFn: fetchCredentials })
  const channels = useQuery({ queryKey: ["channels"], queryFn: fetchChannels })
  const presets = useQuery({ queryKey: ["tls-presets"], queryFn: fetchTlsPresets })
  const cycles = useQuery({
    queryKey: ["credential-cycles", cycleRange.from, cycleRange.to],
    queryFn: () => fetchCredentialCycles(cycleRange.from, cycleRange.to),
  })
  const providerMutation = useMutation({
    mutationFn: ({ value, id }: ProviderMutation) => saveProvider(value, id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["providers"] }),
  })
  const credentialMutation = useMutation({
    mutationFn: ({ value, id }: CredentialMutation) => saveCredential(value, id),
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["credentials"] }),
        queryClient.invalidateQueries({ queryKey: ["credential-cycles"] }),
      ])
    },
  })

  return (
    <ProvidersView
      providers={providers.data ?? []}
      providersLoading={providers.isLoading}
      providersError={providers.isError}
      channels={channels.data ?? []}
      channelsLoading={channels.isLoading}
      channelsError={channels.isError}
      presets={presets.data ?? []}
      presetsLoading={presets.isLoading}
      presetsError={presets.isError}
      credentials={credentials.data ?? []}
      credentialsLoading={credentials.isLoading}
      credentialsError={credentials.isError}
      cycles={cycles.data ?? []}
      cyclesLoading={cycles.isLoading}
      cyclesError={cycles.isError}
      savingProviderId={providerMutation.isPending ? providerMutation.variables?.id ?? null : null}
      savingCredentialId={credentialMutation.isPending ? credentialMutation.variables?.id ?? null : null}
      onSaveProvider={async (value, id) => { await providerMutation.mutateAsync({ value, id }) }}
      onSaveCredential={async (value, id) => { await credentialMutation.mutateAsync({ value, id }) }}
    />
  )
}

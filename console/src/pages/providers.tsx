import type { CredentialWriteRequest } from "@/generated/CredentialWriteRequest"
import type { ProviderWriteRequest } from "@/generated/ProviderWriteRequest"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
  credentials as fetchCredentials,
  providers as fetchProviders,
  saveCredential,
  saveProvider,
  providerRuleSets,
  priceRules,
  priceRates,
  providerModels,
  routingRules,
  ruleSets,
  rules,
} from "@/api/control"
import {
  channels as fetchChannels,
  credentialCycles as fetchCredentialCycles,
  tlsPresets as fetchTlsPresets,
} from "@/api/observability"
import { ProvidersView } from "@/components/providers/providers-view"
import { useRuleMutations } from "@/components/rules/use-rule-mutations"

const MAX_CYCLE_RANGE_SECONDS = 366 * 24 * 60 * 60
const PROVIDER_QUERY_KEYS = [["providers"], ["rule-sets"], ["provider-rule-sets"], ["routing-rules"]]

type ProviderMutation = { value: ProviderWriteRequest; id?: number }
type CredentialMutation = { value: CredentialWriteRequest; id?: number }

export function ProvidersPage() {
  const queryClient = useQueryClient()
  const ruleMutations = useRuleMutations()
  const providers = useQuery({ queryKey: ["providers"], queryFn: fetchProviders })
  const credentials = useQuery({ queryKey: ["credentials"], queryFn: fetchCredentials })
  const channels = useQuery({ queryKey: ["channels"], queryFn: fetchChannels })
  const presets = useQuery({ queryKey: ["tls-presets"], queryFn: fetchTlsPresets })
  const cycles = useQuery({
    queryKey: ["credential-cycles", "providers"],
    queryFn: () => {
      const to = Math.floor(Date.now() / 1000) + 1
      return fetchCredentialCycles(to - MAX_CYCLE_RANGE_SECONDS, to)
    },
    refetchInterval: 30_000,
  })
  const setQuery = useQuery({ queryKey: ["rule-sets"], queryFn: ruleSets })
  const ruleQuery = useQuery({ queryKey: ["rules"], queryFn: rules })
  const attachmentQuery = useQuery({ queryKey: ["provider-rule-sets"], queryFn: providerRuleSets })
  const routingQuery = useQuery({ queryKey: ["routing-rules"], queryFn: routingRules })
  const providerModelQuery = useQuery({ queryKey: ["provider-models"], queryFn: providerModels })
  const priceRuleQuery = useQuery({ queryKey: ["price-rules"], queryFn: priceRules })
  const priceRateQuery = useQuery({ queryKey: ["price-rates"], queryFn: priceRates })
  const providerMutation = useMutation({
    mutationFn: ({ value, id }: ProviderMutation) => saveProvider(value, id),
    onSuccess: () => Promise.all(
      PROVIDER_QUERY_KEYS.map((queryKey) => queryClient.invalidateQueries({ queryKey })),
    ),
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
      ruleSets={setQuery.data ?? []}
      rules={ruleQuery.data ?? []}
      attachments={attachmentQuery.data ?? []}
      routingRules={routingQuery.data ?? []}
      providerModels={providerModelQuery.data ?? []}
      priceRules={priceRuleQuery.data ?? []}
      priceRates={priceRateQuery.data ?? []}
      ruleMutations={ruleMutations}
    />
  )
}

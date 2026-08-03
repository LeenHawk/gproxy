import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { ApiError } from "@/api/http";
import {
  providersQuery,
  upsertProvider,
  type Provider,
  type ProviderListItem,
} from "@/api/providers";

function providerInput(provider: Provider, enabled: boolean) {
  return {
    id: provider.id,
    name: provider.name,
    channel: provider.channel,
    label: provider.label,
    settings_json: provider.settings_json,
    credential_strategy: provider.credential_strategy,
    proxy_url: provider.proxy_url,
    ...(provider.tls_fingerprint != null ? { tls_fingerprint: provider.tls_fingerprint } : {}),
    enabled,
  };
}

/** Provider writes are full upserts, so every setting is round-tripped unchanged. */
export function useProviderToggle() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ provider, enabled }: { provider: Provider; enabled: boolean }) =>
      upsertProvider(providerInput(provider, enabled)),
    onMutate: async ({ provider, enabled }) => {
      await queryClient.cancelQueries({ queryKey: providersQuery.queryKey });
      const previousList = queryClient.getQueryData<ProviderListItem[]>(providersQuery.queryKey);
      const detailKey = ["providers", provider.id] as const;
      const previousDetail = queryClient.getQueryData<Provider>(detailKey);
      queryClient.setQueryData<ProviderListItem[]>(providersQuery.queryKey, (current) =>
        current?.map((item) => (item.id === provider.id ? { ...item, enabled } : item)),
      );
      queryClient.setQueryData<Provider>(detailKey, (current) =>
        current ? { ...current, enabled } : current,
      );
      return { detailKey, previousDetail, previousList };
    },
    onError: (error, _variables, context) => {
      if (context) {
        queryClient.setQueryData(providersQuery.queryKey, context.previousList);
        queryClient.setQueryData(context.detailKey, context.previousDetail);
      }
      toast.error(error instanceof ApiError ? error.message : String(error));
    },
    onSettled: () => void queryClient.invalidateQueries({ queryKey: providersQuery.queryKey }),
  });
}

import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { ApiError } from "@/api/http";
import {
  providerModelsQuery,
  upsertProviderModel,
  type ProviderModel,
} from "@/api/provider-models";

export function useProviderModelToggle(providerId: number) {
  const queryClient = useQueryClient();
  const queryKey = providerModelsQuery(providerId).queryKey;

  return useMutation({
    mutationFn: ({ model, enabled }: { model: ProviderModel; enabled: boolean }) =>
      upsertProviderModel(providerId, {
        id: model.id,
        provider_id: model.provider_id,
        model_id: model.model_id,
        display_name: model.display_name,
        ...(model.variants_json != null ? { variants_json: model.variants_json } : {}),
        context_window: model.context_window,
        max_input_tokens: model.max_input_tokens,
        max_output_tokens: model.max_output_tokens,
        thinking_supported: model.thinking_supported,
        thinking_adaptive_supported: model.thinking_adaptive_supported,
        thinking_enabled_supported: model.thinking_enabled_supported,
        enabled,
      }),
    onMutate: async ({ model, enabled }) => {
      await queryClient.cancelQueries({ queryKey });
      const previous = queryClient.getQueryData<ProviderModel[]>(queryKey);
      queryClient.setQueryData<ProviderModel[]>(queryKey, (current) =>
        current?.map((item) => (item.id === model.id ? { ...item, enabled } : item)),
      );
      return { previous };
    },
    onError: (error, _variables, context) => {
      if (context) queryClient.setQueryData(queryKey, context.previous);
      toast.error(error instanceof ApiError ? error.message : String(error));
    },
    onSettled: () => void queryClient.invalidateQueries({ queryKey }),
  });
}

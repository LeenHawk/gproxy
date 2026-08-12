import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { ApiError } from "@/api/http";
import { priceRulesQuery, upsertPriceRule, type PriceRule } from "@/api/price-rules";

export function usePriceRuleToggle() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ rule, enabled }: { rule: PriceRule; enabled: boolean }) =>
      upsertPriceRule({
        id: rule.id,
        provider_id: rule.provider_id,
        match_type: rule.match_type,
        model_match: rule.model_match,
        input_price: rule.input_price,
        output_price: rule.output_price,
        cache_read_price: rule.cache_read_price,
        cache_creation_5m_price: rule.cache_creation_5m_price,
        cache_creation_30m_price: rule.cache_creation_30m_price,
        cache_creation_1h_price: rule.cache_creation_1h_price,
        image_output_price: rule.image_output_price,
        pricing_tiers_json: rule.pricing_tiers_json,
        enabled,
      }),
    onMutate: async ({ rule, enabled }) => {
      await queryClient.cancelQueries({ queryKey: priceRulesQuery.queryKey });
      const previous = queryClient.getQueryData<PriceRule[]>(priceRulesQuery.queryKey);
      queryClient.setQueryData<PriceRule[]>(priceRulesQuery.queryKey, (current) =>
        current?.map((item) => (item.id === rule.id ? { ...item, enabled } : item)),
      );
      return { previous };
    },
    onError: (error, _variables, context) => {
      if (context) queryClient.setQueryData(priceRulesQuery.queryKey, context.previous);
      toast.error(error instanceof ApiError ? error.message : String(error));
    },
    onSettled: () => void queryClient.invalidateQueries({ queryKey: priceRulesQuery.queryKey }),
  });
}

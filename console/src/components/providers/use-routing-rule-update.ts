import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { ApiError } from "@/api/http";
import { routingRulesQuery, upsertRoutingRule, type RoutingRule } from "@/api/rules";

type RoutingPatch = Pick<RoutingRule, "enabled">;

export function useRoutingRuleUpdate(providerId: number) {
  const queryClient = useQueryClient();
  const queryKey = routingRulesQuery(providerId).queryKey;

  return useMutation({
    mutationFn: ({ rule, patch }: { rule: RoutingRule; patch: RoutingPatch }) => {
      const updated = { ...rule, ...patch };
      return upsertRoutingRule(providerId, {
        id: updated.id,
        provider_id: updated.provider_id,
        operation: updated.operation,
        kind: updated.kind,
        implementation: updated.implementation,
        dest_operation: updated.dest_operation,
        dest_kind: updated.dest_kind,
        sort_order: updated.sort_order,
        enabled: updated.enabled,
      });
    },
    onMutate: async ({ rule, patch }) => {
      await queryClient.cancelQueries({ queryKey });
      const previous = queryClient.getQueryData<RoutingRule[]>(queryKey);
      queryClient.setQueryData<RoutingRule[]>(queryKey, (current) =>
        current?.map((item) => (item.id === rule.id ? { ...item, ...patch } : item)),
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

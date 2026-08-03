import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { ApiError } from "@/api/http";
import { ruleSetQuery, ruleSetsQuery, upsertRuleSet, type RuleSet } from "@/api/rules";

export function useRuleSetToggle() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ ruleSet, enabled }: { ruleSet: RuleSet; enabled: boolean }) =>
      upsertRuleSet({
        id: ruleSet.id,
        name: ruleSet.name,
        description: ruleSet.description,
        enabled,
      }),
    onMutate: async ({ ruleSet, enabled }) => {
      await queryClient.cancelQueries({ queryKey: ["rule-sets"] });
      const previousList = queryClient.getQueryData<RuleSet[]>(ruleSetsQuery.queryKey);
      const previousDetail = queryClient.getQueryData<RuleSet>(ruleSetQuery(ruleSet.id).queryKey);
      queryClient.setQueryData<RuleSet[]>(ruleSetsQuery.queryKey, (current) =>
        current?.map((item) => (item.id === ruleSet.id ? { ...item, enabled } : item)),
      );
      queryClient.setQueryData<RuleSet>(ruleSetQuery(ruleSet.id).queryKey, (current) =>
        current ? { ...current, enabled } : current,
      );
      return { previousList, previousDetail, id: ruleSet.id };
    },
    onError: (error, _variables, context) => {
      if (context) {
        queryClient.setQueryData(ruleSetsQuery.queryKey, context.previousList);
        queryClient.setQueryData(ruleSetQuery(context.id).queryKey, context.previousDetail);
      }
      toast.error(error instanceof ApiError ? error.message : String(error));
    },
    onSettled: () => void queryClient.invalidateQueries({ queryKey: ["rule-sets"] }),
  });
}

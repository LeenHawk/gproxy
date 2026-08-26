import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import type { ProviderRuleSetWriteRequest } from "@/generated/ProviderRuleSetWriteRequest"
import type { RuleSetWriteRequest } from "@/generated/RuleSetWriteRequest"
import type { RuleWriteRequest } from "@/generated/RuleWriteRequest"
import { deleteProviderRuleSet, deleteRule, deleteRuleSet, saveProviderRuleSet, saveRule, saveRuleSet } from "@/api/control"
import type { RuleMutations } from "./rules-workspace"

const KEYS = [["rule-sets"], ["rules"], ["provider-rule-sets"]]

export function useRuleMutations(): RuleMutations {
  const { t } = useTranslation()
  const client = useQueryClient()
  const refresh = () => Promise.all(KEYS.map((queryKey) => client.invalidateQueries({ queryKey })))
  const saved = () => toast.success(t("rules.saved"))
  const failed = () => toast.error(t("rules.saveError"))
  const setMutation = useMutation({ mutationFn: ({ value, id }: { value: RuleSetWriteRequest; id?: number }) => saveRuleSet(value, id), onSuccess: async () => { await refresh(); saved() }, onError: failed })
  const setDelete = useMutation({ mutationFn: deleteRuleSet, onSuccess: refresh, onError: failed })
  const ruleMutation = useMutation({ mutationFn: ({ value, id }: { value: RuleWriteRequest; id?: number }) => saveRule(value, id), onSuccess: async () => { await refresh(); saved() }, onError: failed })
  const ruleDelete = useMutation({ mutationFn: deleteRule, onSuccess: refresh, onError: failed })
  const attachmentMutation = useMutation({ mutationFn: (value: ProviderRuleSetWriteRequest) => saveProviderRuleSet(value), onSuccess: async () => { await refresh(); saved() }, onError: failed })
  const attachmentDelete = useMutation({ mutationFn: deleteProviderRuleSet, onSuccess: refresh, onError: failed })
  const pending = [setMutation, setDelete, ruleMutation, ruleDelete, attachmentMutation, attachmentDelete].some((mutation) => mutation.isPending)
  return {
    saving: pending,
    saveSet: async (value, id) => { await setMutation.mutateAsync({ value, id }) },
    deleteSet: (id) => setDelete.mutate(id),
    saveRule: async (value, id) => { await ruleMutation.mutateAsync({ value, id }) },
    deleteRule: (id) => ruleDelete.mutate(id),
    attach: async (value) => { await attachmentMutation.mutateAsync(value) },
    detach: (id) => attachmentDelete.mutate(id),
  }
}

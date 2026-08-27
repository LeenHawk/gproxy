import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { WandSparklesIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"

import { applyRulePreset, rulePresets } from "@/api/control"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"

const RULE_QUERY_KEYS = [["rule-sets"], ["rules"], ["provider-rule-sets"]]

export function ApplicationPresetButton({ providerId }: { providerId: number }) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const query = useQuery({ queryKey: ["rule-presets"], queryFn: rulePresets })
  const presets = (query.data ?? []).filter((preset) => preset.category === "application")
  const mutation = useMutation({
    mutationFn: (preset: string) => applyRulePreset(providerId, preset),
    onSuccess: async () => {
      await Promise.all(
        RULE_QUERY_KEYS.map((queryKey) => queryClient.invalidateQueries({ queryKey })),
      )
      toast.success(t("rules.presets.applied"))
    },
    onError: () => toast.error(t("rules.presets.applyError")),
  })
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="outline" disabled={query.isLoading || mutation.isPending || query.isError}>
          <WandSparklesIcon data-icon="inline-start" />
          {t(mutation.isPending ? "rules.presets.applying" : "rules.presets.apply")}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuGroup>
          <DropdownMenuLabel>{t("rules.presets.application")}</DropdownMenuLabel>
          {presets.map((preset) => (
            <DropdownMenuItem key={preset.id} onSelect={() => mutation.mutate(preset.id)}>
              {preset.name}
            </DropdownMenuItem>
          ))}
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

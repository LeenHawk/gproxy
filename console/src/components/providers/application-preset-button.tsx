import { useMutation } from "@tanstack/react-query";
import { Check, ChevronDown, WandSparkles } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import type { ProviderRuleSet, RuleSet } from "@/api/rules";
import { ApiError } from "@/api/http";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  APPLICATION_RULE_SET_PRESETS,
  applyApplicationPreset,
  findApplicationPreset,
  type ApplicationRuleSetPreset,
} from "@/lib/application-rule-set-presets";

interface Props {
  providerId: number;
  ruleSets: RuleSet[];
  attachments: ProviderRuleSet[];
  onApplied: (ruleSetId: number) => void;
}

export function ApplicationPresetButton({ providerId, ruleSets, attachments, onApplied }: Props) {
  const { t } = useTranslation("rules");
  const mutation = useMutation({
    mutationFn: (preset: ApplicationRuleSetPreset) =>
      applyApplicationPreset(providerId, preset, ruleSets, attachments).then((ruleSet) => ({ preset, ruleSet })),
    onSuccess: ({ preset, ruleSet }) => {
      toast.success(t("applicationPreset.applied", { name: preset.name }));
      onApplied(ruleSet.id);
    },
    onError: (error) => toast.error(error instanceof ApiError ? error.message : String(error)),
  });

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="outline" size="sm" disabled={mutation.isPending}>
          <WandSparkles className="size-4" />
          {t("applicationPreset.trigger")}
          <ChevronDown className="size-3.5 opacity-60" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-56">
        {APPLICATION_RULE_SET_PRESETS.map((preset) => {
          const ruleSet = findApplicationPreset(ruleSets, preset);
          const attachment = ruleSet
            ? attachments.find((item) => item.rule_set_id === ruleSet.id)
            : undefined;
          const active = Boolean(ruleSet?.enabled && attachment?.enabled);
          return (
            <DropdownMenuItem
              key={preset.id}
              onClick={() => mutation.mutate(preset)}
              disabled={mutation.isPending}
              className="justify-between py-1.5"
            >
              <span>{preset.name}</span>
              <span className="flex items-center gap-1 text-xs text-muted-foreground">
                {active && <Check className="size-3.5" />}
                {t(active ? "applicationPreset.refresh" : "applicationPreset.apply")}
              </span>
            </DropdownMenuItem>
          );
        })}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

import { useMutation } from "@tanstack/react-query";
import { WandSparkles } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import type { ProviderRuleSet, RuleSet } from "@/api/rules";
import { ApiError } from "@/api/http";
import { Button } from "@/components/ui/button";
import { applyOpenCodePreset, findOpenCodePreset } from "@/lib/opencode-rule-set";

interface Props {
  providerId: number;
  ruleSets: RuleSet[];
  attachments: ProviderRuleSet[];
  onApplied: (ruleSetId: number) => void;
}

export function OpenCodeRuleSetButton({ providerId, ruleSets, attachments, onApplied }: Props) {
  const { t } = useTranslation("rules");
  const preset = findOpenCodePreset(ruleSets);
  const attached = preset && attachments.some((attachment) => attachment.rule_set_id === preset.id);
  const mutation = useMutation({
    mutationFn: () => applyOpenCodePreset(providerId, ruleSets, attachments),
    onSuccess: (ruleSet) => {
      toast.success(t("opencodePreset.applied"));
      onApplied(ruleSet.id);
    },
    onError: (error) => toast.error(error instanceof ApiError ? error.message : String(error)),
  });

  return (
    <Button variant="outline" size="sm" onClick={() => mutation.mutate()} disabled={mutation.isPending}>
      <WandSparkles className="size-4" />
      {t(attached ? "opencodePreset.refresh" : "opencodePreset.apply")}
    </Button>
  );
}

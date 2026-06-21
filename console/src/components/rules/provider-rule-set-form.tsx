import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { upsertProviderRuleSet, type ProviderRuleSet, type RuleSet } from "@/api/rules";
import { ApiError } from "@/api/http";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";

interface Props {
  providerId: number;
  attachment?: ProviderRuleSet;
  ruleSets: RuleSet[];
  /** IDs of rule sets already attached — excluded from the create dropdown. */
  attachedIds?: number[];
  onSaved?: (prs: ProviderRuleSet) => void;
}

export function ProviderRuleSetForm({ providerId, attachment, ruleSets, attachedIds = [], onSaved }: Props) {
  const { t } = useTranslation("rules");
  const qc = useQueryClient();

  const availableForCreate = attachment
    ? ruleSets
    : ruleSets.filter((rs) => !attachedIds.includes(rs.id));

  const defaultRuleSetId = attachment?.rule_set_id ?? availableForCreate[0]?.id ?? 0;

  const [ruleSetId, setRuleSetId] = useState<number>(defaultRuleSetId);
  const [sortOrder, setSortOrder] = useState(String(attachment?.sort_order ?? 0));
  const [enabled, setEnabled] = useState(attachment?.enabled ?? true);
  const [formError, setFormError] = useState<string | null>(null);

  const mutation = useMutation({
    mutationFn: () => {
      const orderNum = Number(sortOrder);
      if (!Number.isFinite(orderNum)) throw new ApiError(0, "bad_request", t("validation.sortOrderRequired"));
      if (!ruleSetId) throw new ApiError(0, "bad_request", t("validation.nameRequired"));

      return upsertProviderRuleSet(providerId, {
        id: attachment?.id ?? null,
        provider_id: providerId,
        rule_set_id: ruleSetId,
        sort_order: orderNum,
        enabled,
      });
    },
    onSuccess: (result) => {
      void qc.invalidateQueries({ queryKey: ["providers", providerId, "rule-sets"] });
      toast.success(t("common.save"));
      setFormError(null);
      onSaved?.(result);
    },
    onError: (e) => {
      setFormError(e instanceof ApiError ? e.message : String(e));
    },
  });

  const displayRuleSets = attachment
    ? ruleSets
    : availableForCreate;

  return (
    <form
      className="grid gap-4"
      onSubmit={(e) => { e.preventDefault(); setFormError(null); mutation.mutate(); }}
    >
      <div className="grid gap-1">
        <Label htmlFor="prs-ruleset">{t("providerRuleSet.ruleSet")}</Label>
        <Select
          value={String(ruleSetId)}
          onValueChange={(v) => setRuleSetId(Number(v))}
          disabled={!!attachment}
        >
          <SelectTrigger id="prs-ruleset"><SelectValue /></SelectTrigger>
          <SelectContent>
            {displayRuleSets.map((rs) => (
              <SelectItem key={rs.id} value={String(rs.id)}>{rs.name}</SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      <div className="grid gap-1">
        <Label htmlFor="prs-sort">{t("providerRuleSet.sortOrder")}</Label>
        <Input
          id="prs-sort"
          type="number"
          value={sortOrder}
          onChange={(e) => setSortOrder(e.target.value)}
        />
      </div>

      <div className="flex items-center justify-between gap-4">
        <Label htmlFor="prs-enabled">{t("providerRuleSet.enabled")}</Label>
        <Switch id="prs-enabled" checked={enabled} onCheckedChange={setEnabled} />
      </div>

      {formError && (
        <p role="alert" className="text-sm text-destructive">{formError}</p>
      )}
      <Button type="submit" disabled={mutation.isPending || (!attachment && displayRuleSets.length === 0)}>
        {mutation.isPending ? "…" : t("common.save")}
      </Button>
    </form>
  );
}

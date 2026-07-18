import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { upsertRule, type Rule } from "@/api/rules";
import { ApiError } from "@/api/http";
import { normalizeRuleConfig } from "@/lib/rule-config";
import { ModelPatternField, OperationChips, toOperationArray, fromOperationArray } from "./filter-fields";
import { RuleConfigFields } from "./rule-config-fields";
import { RuleKindPicker } from "./rule-kind-picker";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";

interface Props {
  ruleSetId: number;
  rule?: Rule;
  modelOptions?: string[];
  onSaved?: (r: Rule) => void;
}

function validateConfig(kind: string, cfg: unknown, t: (k: string) => string): string | null {
  const c = cfg as Record<string, unknown> | null | undefined;
  if (!c) return t("validation.configInvalid");
  if (kind === "system_text" && !c.text) return t("validation.configTextRequired");
  if (kind === "rewrite" && !c.path) return t("validation.configPathRequired");
  if (kind === "rewrite" && !c.action) return t("validation.configActionRequired");
  if (kind === "transform" && (!c.locate || !Array.isArray(c.actions) || c.actions.length === 0)) {
    return t("validation.configTransformRequired");
  }
  if (kind === "cache_breakpoint" && !c.target) return t("validation.configTargetRequired");
  if (kind === "cache_breakpoint" && c.index === 0) return t("validation.cacheIndexZero");
  if (kind === "header" && !c.name) return t("validation.configHeaderNameRequired");
  return null;
}

export function RuleForm({ ruleSetId, rule, modelOptions, onSaved }: Props) {
  const { t } = useTranslation("rules");
  const qc = useQueryClient();
  const initialKind = rule?.kind ?? "transform";
  const initialConfig = normalizeRuleConfig(initialKind, rule?.config_json);

  const [kind, setKind] = useState(initialKind);
  const [sortOrder, setSortOrder] = useState(String(rule?.sort_order ?? 0));
  const [enabled, setEnabled] = useState(rule?.enabled ?? true);
  const [filterModelPattern, setFilterModelPattern] = useState(rule?.filter_model_pattern ?? "");
  const [ops, setOps] = useState<string[]>(toOperationArray(rule?.filter_operation_keys));
  const [configValue, setConfigValue] = useState<unknown>(initialConfig);
  const [configValid, setConfigValid] = useState(true);
  const [formError, setFormError] = useState<string | null>(null);
  const [drafts, setDrafts] = useState<Record<string, unknown>>({ [initialKind]: initialConfig });

  const handleKindChange = (k: string) => {
    setDrafts((d) => ({ ...d, [kind]: configValue })); // stash current
    setKind(k);
    setConfigValue(normalizeRuleConfig(k, drafts[k]));  // restore or initialize
    setConfigValid(true);
  };

  const mutation = useMutation({
    mutationFn: () => {
      const orderNum = Number(sortOrder);
      if (!Number.isFinite(orderNum)) throw new ApiError(0, "bad_request", t("validation.sortOrderRequired"));
      if (!configValid) throw new ApiError(0, "bad_request", t("validation.configInvalid"));
      const normalizedConfig = normalizeRuleConfig(kind, configValue);
      const cfgErr = validateConfig(kind, normalizedConfig, t);
      if (cfgErr) throw new ApiError(0, "bad_request", cfgErr);

      return upsertRule(ruleSetId, {
        id: rule?.id ?? null,
        rule_set_id: ruleSetId,
        kind,
        config_json: normalizedConfig,
        filter_model_pattern: filterModelPattern.trim() || null,
        filter_operation_keys: fromOperationArray(ops),
        sort_order: orderNum,
        enabled,
      });
    },
    onSuccess: (result) => {
      void qc.invalidateQueries({ queryKey: ["rule-sets", ruleSetId, "rules"] });
      toast.success(t("common.save"));
      setFormError(null);
      onSaved?.(result);
    },
    onError: (e) => {
      setFormError(e instanceof ApiError ? e.message : String(e));
    },
  });

  return (
    <form
      className="grid gap-4"
      onSubmit={(e) => { e.preventDefault(); setFormError(null); mutation.mutate(); }}
    >
      <div className="grid gap-1">
        <Label>{t("rule.kind")}</Label>
        <RuleKindPicker value={kind} onChange={handleKindChange} />
      </div>
      <div className="grid gap-1">
        <Label htmlFor="rule-sort">{t("rule.sortOrder")}</Label>
        <Input
          id="rule-sort"
          type="number"
          value={sortOrder}
          onChange={(e) => setSortOrder(e.target.value)}
        />
      </div>
      <div className="flex items-center justify-between gap-4">
        <Label htmlFor="rule-enabled">{t("rule.enabled")}</Label>
        <Switch id="rule-enabled" checked={enabled} onCheckedChange={setEnabled} />
      </div>
      <ModelPatternField value={filterModelPattern} onChange={setFilterModelPattern} modelOptions={modelOptions} />
      <OperationChips value={ops} onChange={setOps} />
      <div className="grid gap-1">
        <Label>{t("rule.configJson")}</Label>
        <RuleConfigFields
          key={kind}
          kind={kind}
          value={configValue}
          onChange={setConfigValue}
          onValidChange={setConfigValid}
        />
      </div>
      {formError && (
        <p role="alert" className="text-sm text-destructive">{formError}</p>
      )}
      <Button type="submit" disabled={mutation.isPending}>
        {mutation.isPending ? "…" : t("common.save")}
      </Button>
    </form>
  );
}

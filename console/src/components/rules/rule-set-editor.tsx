import { useState } from "react";
import { useMutation, useQueries, useQuery, useQueryClient } from "@tanstack/react-query";
import { Plus, Trash2, Pencil } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  ruleSetQuery, rulesQuery, providerRuleSetsQuery, deleteRule, upsertRule,
  type Rule, type ProviderRuleSet,
} from "@/api/rules";
import { providersQuery } from "@/api/providers";
import { providerModelsQuery } from "@/api/provider-models";
import { ApiError } from "@/api/http";
import { computeRuleSetUsage, groupRulesByKind } from "@/lib/rule-usage";
import { RULE_KIND_META, summarizeRuleConfig } from "./rule-kind-meta";
import { RuleForm } from "./rule-form";
import { RuleSetForm } from "./rule-set-form";
import { ConfirmDangerous } from "@/components/confirm-dangerous";
import { EntityDialog } from "@/components/entity-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { Switch } from "@/components/ui/switch";

export function RuleSetEditor({ ruleSetId, providerId }: { ruleSetId: number; providerId?: number }) {
  const { t } = useTranslation("rules");
  const { t: tCommon } = useTranslation("common");
  const qc = useQueryClient();
  const { data: ruleSet } = useQuery(ruleSetQuery(ruleSetId));
  const { data: rules = [] } = useQuery(rulesQuery(ruleSetId));
  const { data: providers = [] } = useQuery(providersQuery);
  const { data: models = [] } = useQuery({ ...providerModelsQuery(providerId ?? 0), enabled: !!providerId });

  // Aggregate attachments across all providers (N+1, admin scale) for shared-impact.
  const attachQueries = useQueries({
    queries: providers.map((p) => providerRuleSetsQuery(p.id)),
  });
  const allAttachments: ProviderRuleSet[] = attachQueries.flatMap((q) => q.data ?? []);
  const usage = computeRuleSetUsage(ruleSetId, allAttachments);
  const otherCount = usage.providerIds.filter((id) => id !== providerId).length;

  const [editMeta, setEditMeta] = useState(false);
  const [addOpen, setAddOpen] = useState(false);
  const [editRule, setEditRule] = useState<Rule | null>(null);
  const [delRule, setDelRule] = useState<Rule | null>(null);

  const modelOptions = models.map((m) => m.model_id);
  const groups = groupRulesByKind(rules);

  const invalidateRules = () => qc.invalidateQueries({ queryKey: ["rule-sets", ruleSetId, "rules"] });

  const removal = useMutation({
    mutationFn: (id: number) => deleteRule(id),
    onSuccess: () => { void invalidateRules(); setDelRule(null); },
    onError: (e) => { toast.error(e instanceof ApiError ? e.message : String(e)); setDelRule(null); },
  });

  const toggleEnabled = useMutation({
    mutationFn: ({ r, next }: { r: Rule; next: boolean }) =>
      upsertRule(ruleSetId, {
        id: r.id,
        rule_set_id: r.rule_set_id,
        kind: r.kind,
        config_json: r.config_json,
        filter_model_pattern: r.filter_model_pattern,
        filter_operation_keys: r.filter_operation_keys,
        sort_order: r.sort_order,
        enabled: next,
      }),
    onSuccess: () => void invalidateRules(),
    onError: (e) => toast.error(e instanceof ApiError ? e.message : String(e)),
  });

  if (!ruleSet) return null;

  return (
    <div className="grid gap-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          <h3 className="font-semibold">{ruleSet.name}</h3>
          <Badge variant={ruleSet.enabled ? "secondary" : "outline"}>{ruleSet.enabled ? "on" : "off"}</Badge>
          {usage.scope === "shared" && <Badge variant="outline">{t("usage.shared", { count: usage.providerIds.length })}</Badge>}
        </div>
        <Button variant="outline" size="sm" onClick={() => setEditMeta(true)}><Pencil className="size-4" /></Button>
      </div>

      {otherCount > 0 && (
        <p className="rounded-md border border-amber-500/40 bg-amber-500/5 px-3 py-2 text-xs text-amber-700 dark:text-amber-400">
          {t("usage.sharedWarning", { count: otherCount })}
        </p>
      )}

      <p className="text-xs text-muted-foreground">{t("orderHint")}</p>

      {groups.length === 0 && <p className="text-sm text-muted-foreground">{t("rule.empty")}</p>}
      {groups.map((g) => {
        const Icon = RULE_KIND_META[g.kind]?.icon;
        return (
          <section key={g.kind} className="grid gap-1.5">
            <div className="flex items-center gap-2 text-sm font-medium">
              {Icon && <Icon className="size-4" aria-hidden />} {t(`kind.${g.kind}`)}
            </div>
            {g.rules.map((r) => (
              <div key={r.id} className="flex items-center justify-between gap-2 rounded-md border px-3 py-1.5">
                <button type="button" className="min-w-0 flex-1 text-left" onClick={() => setEditRule(r)}>
                  <span className="block truncate font-mono text-xs">{summarizeRuleConfig(r.kind, r.config_json)}</span>
                  <span className="text-[10px] text-muted-foreground">
                    {r.filter_model_pattern ?? t("pipeline.allRequests")}
                  </span>
                </button>
                <Switch
                  checked={r.enabled}
                  onCheckedChange={(next) => toggleEnabled.mutate({ r, next })}
                  aria-label={t("rule.enabled")}
                />
                <Button variant="ghost" size="icon" className="text-destructive" onClick={() => setDelRule(r)}>
                  <Trash2 className="size-4" />
                </Button>
              </div>
            ))}
          </section>
        );
      })}

      <Separator />
      <div className="flex flex-wrap gap-2">
        <Button size="sm" variant="outline" onClick={() => setAddOpen(true)}>
          <Plus className="size-4" /> {t("rule.add")}
        </Button>
      </div>

      <EntityDialog open={editMeta} onOpenChange={setEditMeta} title={ruleSet.name}>
        <RuleSetForm ruleSet={ruleSet} onSaved={() => { setEditMeta(false); void qc.invalidateQueries({ queryKey: ["rule-sets", ruleSetId] }); }} />
      </EntityDialog>
      <EntityDialog open={addOpen} onOpenChange={setAddOpen} title={t("rule.add")} wide>
        <RuleForm ruleSetId={ruleSetId} modelOptions={modelOptions} onSaved={() => { setAddOpen(false); void invalidateRules(); }} />
      </EntityDialog>
      <EntityDialog open={editRule !== null} onOpenChange={(o) => { if (!o) setEditRule(null); }}
        title={editRule ? t(`kind.${editRule.kind}`) : ""} wide>
        {editRule && <RuleForm ruleSetId={ruleSetId} rule={editRule} modelOptions={modelOptions}
          onSaved={() => { setEditRule(null); void invalidateRules(); }} />}
      </EntityDialog>
      <ConfirmDangerous open={delRule !== null} onOpenChange={(o) => { if (!o) setDelRule(null); }}
        title={delRule ? t(`kind.${delRule.kind}`) : ""} description={t("rule.deleteConfirm")}
        confirmLabel={tCommon("actions.delete")} onConfirm={() => delRule && removal.mutate(delRule.id)} pending={removal.isPending} />
    </div>
  );
}

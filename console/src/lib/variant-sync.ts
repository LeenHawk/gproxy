import { api } from "@/api/http";
import { upsertRule, deleteRule, type Rule } from "@/api/rules";
import type { SuffixAction } from "@/components/providers/suffix-presets";
import {
  ensureProviderDefaultRuleSet,
  findProviderDefaultRuleSet,
} from "./provider-rule-set";

/** Full variant names from a model's `variants_json` (bare array or {variants} object). */
export function parseVariantNames(variants: unknown): string[] {
  if (Array.isArray(variants)) return variants.map(String);
  if (variants && typeof variants === "object") {
    const o = variants as { variants?: unknown };
    if (Array.isArray(o.variants)) return o.variants.map(String);
  }
  return [];
}

/** Read the effective preset actions stored as rewrite rules for each variant. */
export function extractVariantActions(
  variantNames: string[],
  rules: Rule[],
): Map<string, SuffixAction[]> {
  const names = new Set(variantNames);
  const actions = new Map<string, SuffixAction[]>();
  const sortedRules = [...rules].sort((a, b) => a.sort_order - b.sort_order || a.id - b.id);

  for (const rule of sortedRules) {
    const name = rule.filter_model_pattern;
    if (!rule.enabled || rule.kind !== "rewrite" || name == null || !names.has(name)) continue;
    if (!rule.config_json || typeof rule.config_json !== "object" || Array.isArray(rule.config_json)) continue;

    const config = rule.config_json as Record<string, unknown>;
    if (config.action !== "set" || typeof config.path !== "string" || !Object.hasOwn(config, "value_json")) continue;
    const current = actions.get(name) ?? [];
    current.push({ path: config.path, value: config.value_json });
    actions.set(name, current);
  }
  return actions;
}

export async function loadVariantActions(
  providerId: number,
  variantNames: string[],
): Promise<Map<string, SuffixAction[]>> {
  if (variantNames.length === 0) return new Map();
  const rsId = await findProviderDefaultRuleSet(providerId);
  if (rsId == null) return new Map();
  const rules = await api<Rule[]>(`/admin/rule-sets/${rsId}/rules`);
  return extractVariantActions(variantNames, rules);
}

export interface RuleDraft {
  kind: "rewrite";
  config_json: { path: string; action: "set"; value_json: unknown };
  filter_model_pattern: string;
  filter_operation_keys: null;
  sort_order: number;
  enabled: true;
}

export interface VariantRulePlan {
  toDelete: number[];
  toCreate: RuleDraft[];
}

interface PlanArgs {
  oldNames: string[];
  newNames: string[];
  presetActions: Map<string, SuffixAction[]>;
  existingRules: Rule[];
}

/** Pure: decide which dedicated-set rules to delete and create for variant changes.
 *  Rules are keyed by the variant's full name (filter_model_pattern == name).
 *  - Removed names (old∖new): delete their rules.
 *  - Names whose behavior was (re)set this session: delete then recreate (idempotent
 *    overwrite), one rewrite rule per action. */
export function planVariantRuleChanges(a: PlanArgs): VariantRulePlan {
  const newSet = new Set(a.newNames);
  const removed = a.oldNames.filter((n) => !newSet.has(n));

  const rekeyed = new Set<string>([...removed, ...a.presetActions.keys()]);
  const toDelete = a.existingRules
    .filter((r) => r.filter_model_pattern != null && rekeyed.has(r.filter_model_pattern))
    .map((r) => r.id);

  const toCreate: RuleDraft[] = [];
  for (const [name, actions] of a.presetActions) {
    if (!newSet.has(name)) continue; // only for names that actually remain
    actions.forEach((act, i) => {
      toCreate.push({
        kind: "rewrite",
        config_json: { path: act.path, action: "set", value_json: act.value },
        filter_model_pattern: name,
        filter_operation_keys: null,
        sort_order: i,
        enabled: true,
      });
    });
  }
  return { toDelete, toCreate };
}

interface SyncArgs {
  providerId: number;
  providerName: string;
  oldNames: string[];
  newNames: string[];
  presetActions: Map<string, SuffixAction[]>;
}

/** Apply variant rule changes against the provider's dedicated rule set.
 *  No-op when there is nothing to add or remove (never creates an empty set).
 *
 *  Known limitation (single-user deploy): renaming/deleting a variant leaves its
 *  rule orphaned here — rules are keyed by the full variant name and are not
 *  re-keyed/cleaned on rename. Harmless: an orphan literal pattern matches no
 *  live request. */
export async function syncModelVariants(a: SyncArgs): Promise<void> {
  const newSet = new Set(a.newNames);
  const removed = a.oldNames.filter((n) => !newSet.has(n));
  if (a.presetActions.size === 0 && removed.length === 0) return;

  const rsId = a.presetActions.size > 0
    ? await ensureProviderDefaultRuleSet(a.providerId, a.providerName)
    : await findProviderDefaultRuleSet(a.providerId);
  if (rsId == null) return; // removal-only and no dedicated set exists → nothing to clean

  const existingRules = await api<Rule[]>(`/admin/rule-sets/${rsId}/rules`);
  const plan = planVariantRuleChanges({ ...a, existingRules });

  for (const id of plan.toDelete) await deleteRule(id);
  for (const d of plan.toCreate) {
    await upsertRule(rsId, { rule_set_id: rsId, ...d });
  }
}

import { useEffect, useMemo, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Check, Loader2, Search } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { ApiError } from "@/api/http";
import { upsertPriceRule, type PriceRule, type PriceRuleInput } from "@/api/price-rules";
import openrouterPriceRules from "@/data/openrouter-price-rules.json";
import { EntityDialog } from "@/components/entity-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";

type PriceRuleDraft = Omit<PriceRuleInput, "id">;

interface PriceRuleBundle {
  schema_version: number;
  price_rules: PriceRuleDraft[];
}

interface ImportError {
  model: string;
  error: string;
}

const DEFAULT_BUNDLE = openrouterPriceRules as PriceRuleBundle;
const DEFAULT_RULES = DEFAULT_BUNDLE.price_rules.map((rule) => ({
  ...rule,
  provider_id: null,
  match_type: "contains" as const,
}));

function ruleKey(rule: Pick<PriceRuleInput, "provider_id" | "match_type" | "model_match">): string {
  return [
    rule.provider_id ?? "",
    rule.match_type,
    rule.model_match,
  ].join("\u0000");
}

export function DefaultPriceRuleImport({
  open,
  onOpenChange,
  existingRules,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  existingRules: PriceRule[];
}) {
  const { t } = useTranslation("pricing");
  const queryClient = useQueryClient();
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [search, setSearch] = useState("");
  const [done, setDone] = useState(0);
  const [total, setTotal] = useState(0);
  const [errors, setErrors] = useState<ImportError[]>([]);

  useEffect(() => {
    if (open) {
      setSelected(new Set(DEFAULT_RULES.map(ruleKey)));
      setSearch("");
      setDone(0);
      setTotal(0);
      setErrors([]);
    }
  }, [open]);

  const existingByKey = useMemo(
    () => new Map(existingRules.map((rule) => [ruleKey(rule), rule])),
    [existingRules],
  );

  const term = search.trim().toLowerCase();
  const visibleRules = term
    ? DEFAULT_RULES.filter((rule) => rule.model_match.toLowerCase().includes(term))
    : DEFAULT_RULES;
  const visibleKeys = visibleRules.map(ruleKey);
  const allVisibleSelected = visibleKeys.length > 0 && visibleKeys.every((key) => selected.has(key));
  const selectedRules = DEFAULT_RULES.filter((rule) => selected.has(ruleKey(rule)));
  const selectedOverwriteCount = selectedRules.filter((rule) => existingByKey.has(ruleKey(rule))).length;

  const toggle = (key: string) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });

  const toggleAllVisible = () =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (allVisibleSelected) visibleKeys.forEach((key) => next.delete(key));
      else visibleKeys.forEach((key) => next.add(key));
      return next;
    });

  const importMutation = useMutation({
    mutationFn: async () => {
      setDone(0);
      setTotal(selectedRules.length);
      setErrors([]);
      const failures: ImportError[] = [];

      for (let i = 0; i < selectedRules.length; i++) {
        const rule = selectedRules[i];
        try {
          await upsertPriceRule({
            ...rule,
            id: existingByKey.get(ruleKey(rule))?.id ?? null,
          });
        } catch (error) {
          failures.push({
            model: rule.model_match,
            error: error instanceof ApiError ? error.message : String(error),
          });
        }
        setDone(i + 1);
        setErrors([...failures]);
      }

      return { attempted: selectedRules.length, failed: failures.length };
    },
    onSuccess: async ({ attempted, failed }) => {
      await queryClient.invalidateQueries({ queryKey: ["price-rules"] });
      if (failed > 0) {
        toast.warning(t("defaults.importPartial", { ok: attempted - failed, fail: failed }));
      } else {
        toast.success(t("defaults.imported", { count: attempted }));
        onOpenChange(false);
      }
    },
    onError: (error) => toast.error(error instanceof ApiError ? error.message : String(error)),
  });

  const busy = importMutation.isPending;

  return (
    <EntityDialog
      open={open}
      onOpenChange={onOpenChange}
      title={t("defaults.title")}
      description={t("defaults.description")}
      wide
    >
      <div className="grid gap-3">
        <div className="relative">
          <Search
            className="pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground"
            aria-hidden
          />
          <Input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={t("defaults.search")}
            className="pl-8"
            disabled={busy}
          />
        </div>

        <div className="flex items-center justify-between gap-3">
          <button
            type="button"
            className="text-xs text-primary hover:underline disabled:opacity-50"
            onClick={toggleAllVisible}
            disabled={visibleKeys.length === 0 || busy}
          >
            {allVisibleSelected ? t("defaults.selectNone") : t("defaults.selectAll")}
          </button>
          <span className="text-xs text-muted-foreground">
            {term
              ? t("defaults.shown", { shown: visibleRules.length, total: DEFAULT_RULES.length })
              : t("defaults.count", { total: DEFAULT_RULES.length, selected: selected.size })}
          </span>
        </div>

        <div className="max-h-[48vh] divide-y overflow-y-auto rounded-md border">
          {visibleRules.length === 0 ? (
            <p className="py-6 text-center text-sm text-muted-foreground">{t("defaults.noMatch")}</p>
          ) : (
            visibleRules.map((rule) => {
              const key = ruleKey(rule);
              const checked = selected.has(key);
              const existing = existingByKey.has(key);
              return (
                <button
                  key={key}
                  type="button"
                  onClick={() => toggle(key)}
                  disabled={busy}
                  className={cn(
                    "flex w-full items-center gap-3 px-3 py-2 text-left text-sm disabled:opacity-60",
                    !busy && "hover:bg-accent/50",
                    checked && "bg-primary/5",
                  )}
                >
                  <span
                    className={cn(
                      "grid size-4 shrink-0 place-items-center rounded border",
                      checked ? "border-primary bg-primary text-primary-foreground" : "border-input",
                    )}
                  >
                    {checked && <Check className="size-3" aria-hidden />}
                  </span>
                  <span className="min-w-0 flex-1 truncate font-mono text-xs">{rule.model_match}</span>
                  <span className="shrink-0 text-xs text-muted-foreground">{rule.input_price} / {rule.output_price}</span>
                  {existing && <Badge variant="outline">{t("defaults.overwrite")}</Badge>}
                </button>
              );
            })
          )}
        </div>

        {busy && (
          <p className="text-sm text-muted-foreground" role="status" aria-live="polite">
            {t("defaults.importing", { done, total })}
          </p>
        )}

        {errors.length > 0 && (
          <div role="status" aria-live="assertive" className="grid gap-1">
            {errors.map((failure) => (
              <p key={failure.model} className="font-mono text-xs text-destructive">
                {failure.model}: {failure.error}
              </p>
            ))}
          </div>
        )}

        <div className="flex justify-end gap-2">
          <Button variant="outline" onClick={() => onOpenChange(false)} disabled={busy}>
            {t("defaults.close")}
          </Button>
          <Button
            disabled={busy || selectedRules.length === 0}
            onClick={() => importMutation.mutate()}
          >
            {busy && <Loader2 className="mr-2 size-4 animate-spin" aria-hidden />}
            {busy
              ? t("defaults.importing", { done, total })
              : t("defaults.import", {
                  count: selectedRules.length,
                  overwrite: selectedOverwriteCount,
                })}
          </Button>
        </div>
      </div>
    </EntityDialog>
  );
}

import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Loader2, Search } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { ApiError } from "@/api/http";
import {
  upstreamModelsQuery,
  upsertProviderModel,
  type ProviderModel,
  type UpstreamModel,
} from "@/api/provider-models";
import { upsertPriceRule, type PriceRule } from "@/api/price-rules";
import { EntityDialog } from "@/components/entity-dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ModelPullList } from "@/components/providers/model-pull-list";
import { ModelPullPriceOption } from "@/components/providers/model-pull-price-option";
import {
  defaultPriceInput,
  missingMetadataCount,
  modelSyncInput,
} from "@/components/providers/model-pull-sync";

/** Add new upstream models, fill missing metadata and optionally add default prices. */
export function ModelPullDialog({
  providerId,
  existingModels,
  priceRules,
  open,
  onOpenChange,
}: {
  providerId: number;
  existingModels: ProviderModel[];
  priceRules: PriceRule[];
  open: boolean;
  onOpenChange: (o: boolean) => void;
}) {
  const { t } = useTranslation("providers");
  const qc = useQueryClient();
  const q = useQuery(upstreamModelsQuery(providerId));
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [search, setSearch] = useState("");
  const [importPrices, setImportPrices] = useState(true);

  useEffect(() => {
    if (open) {
      setSelected(new Set());
      setSearch("");
      setImportPrices(true);
      void q.refetch();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const models = q.data ?? [];
  const baseActions = useMemo(() => {
    const existingById = new Map(existingModels.map((model) => [model.model_id, model]));
    return new Map(models.map((model) => {
      const existing = existingById.get(model.id);
      const metadata = missingMetadataCount(existing, model);
      return [model.id, {
        existing,
        metadata,
        price: defaultPriceInput(providerId, model.id, priceRules),
        modelWrite: existing == null || metadata > 0,
      }];
    }));
  }, [existingModels, models, priceRules, providerId]);
  const actionFor = (model: UpstreamModel) => {
    const base = baseActions.get(model.id);
    if (!base) throw new Error(`missing pull action for ${model.id}`);
    return {
      ...base,
      actionable: base.modelWrite || (importPrices && base.price != null),
    };
  };
  const actionableModels = models.filter((model) => actionFor(model).actionable);

  const term = search.trim().toLowerCase();
  const visible = term
    ? models.filter(
        (m) =>
          m.id.toLowerCase().includes(term) ||
          (m.display_name ?? "").toLowerCase().includes(term),
      )
    : models;
  // "Select all" acts on the new+visible set, so you can search → select-all →
  // search again, accumulating selections across filters.
  const visibleActionable = visible.filter((model) => actionFor(model).actionable);
  const allVisibleSelected =
    visibleActionable.length > 0 && visibleActionable.every((model) => selected.has(model.id));

  const toggle = (id: string) =>
    setSelected((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });
  const toggleAll = () =>
    setSelected((s) => {
      const n = new Set(s);
      if (allVisibleSelected) visibleActionable.forEach((model) => n.delete(model.id));
      else visibleActionable.forEach((model) => n.add(model.id));
      return n;
    });

  const selectedModels = actionableModels.filter((model) => selected.has(model.id));

  const importMut = useMutation({
    mutationFn: async () => {
      let added = 0;
      let enriched = 0;
      let priced = 0;
      for (const model of selectedModels) {
        const action = actionFor(model);
        if (action.modelWrite) {
          await upsertProviderModel(
            providerId,
            modelSyncInput(providerId, model, action.existing),
          );
          if (action.existing) enriched += 1;
          else added += 1;
        }
        if (importPrices && action.price) {
          await upsertPriceRule(action.price);
          priced += 1;
        }
      }
      return { added, enriched, priced };
    },
    onSuccess: ({ added, enriched, priced }) => {
      toast.success(t("models.synced", { added, enriched, priced }));
      onOpenChange(false);
    },
    onError: (e) => toast.error(e instanceof ApiError ? e.message : String(e)),
    onSettled: async () => {
      await Promise.all([
        qc.invalidateQueries({ queryKey: ["providers", providerId, "models"] }),
        qc.invalidateQueries({ queryKey: ["price-rules"] }),
      ]);
      setSelected(new Set());
    },
  });

  const err = q.error as ApiError | null;

  return (
    <EntityDialog open={open} onOpenChange={onOpenChange} title={t("models.pullTitle")} wide>
      <div className="grid gap-3">
        {q.isFetching ? (
          <div className="flex items-center justify-center gap-2 py-8 text-sm text-muted-foreground">
            <Loader2 className="size-4 animate-spin" aria-hidden /> {t("models.pulling")}
          </div>
        ) : q.isError ? (
          <p className="rounded-md border border-destructive bg-destructive/10 p-3 text-sm text-destructive">
            {err?.message ?? t("models.pullError")}
          </p>
        ) : models.length === 0 ? (
          <p className="py-6 text-center text-sm text-muted-foreground">{t("models.pullEmpty")}</p>
        ) : (
          <>
            <div className="relative">
              <Search
                className="pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground"
                aria-hidden
              />
              <Input
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder={t("models.pullSearch")}
                className="pl-8"
              />
            </div>
            <div className="flex items-center justify-between">
              <button
                type="button"
                className="text-xs text-primary hover:underline disabled:opacity-50"
                onClick={toggleAll}
                disabled={visibleActionable.length === 0}
              >
                {allVisibleSelected ? t("models.selectNone") : t("models.selectAll")}
              </button>
              <span className="text-xs text-muted-foreground">
                {term
                  ? t("models.pullShown", { shown: visible.length, total: models.length })
                  : t("models.pullCount", { total: models.length, actionable: actionableModels.length })}
              </span>
            </div>
            <ModelPullPriceOption
              checked={importPrices}
              onCheckedChange={setImportPrices}
              disabled={importMut.isPending}
            />
            <div className="max-h-[50vh] divide-y overflow-y-auto rounded-md border">
              <ModelPullList
                models={visible}
                selected={selected}
                importPrices={importPrices}
                pending={importMut.isPending}
                actionFor={(model) => {
                  const action = actionFor(model);
                  return {
                    existing: action.existing != null,
                    metadata: action.metadata,
                    price: action.price != null,
                    actionable: action.actionable,
                  };
                }}
                onToggle={toggle}
              />
            </div>
            <Button
              disabled={selectedModels.length === 0 || importMut.isPending}
              onClick={() => importMut.mutate()}
            >
              {importMut.isPending && <Loader2 className="mr-2 size-4 animate-spin" aria-hidden />}
              {t("models.sync", { count: selectedModels.length })}
            </Button>
          </>
        )}
      </div>
    </EntityDialog>
  );
}

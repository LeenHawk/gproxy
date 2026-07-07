import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { BadgeDollarSign, DownloadCloud, Pencil, Plus, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { deleteProviderModel, providerModelsQuery, type ProviderModel } from "@/api/provider-models";
import { priceRulesQuery, type PriceRule } from "@/api/price-rules";
import type { Provider } from "@/api/providers";
import { ApiError } from "@/api/http";
import { BatchToolbar } from "@/components/batch-toolbar";
import { ConfirmDangerous } from "@/components/confirm-dangerous";
import { DataTable, type DataColumn } from "@/components/data-table";
import { EntityDialog } from "@/components/entity-dialog";
import { PriceRuleForm } from "@/components/pricing/price-rule-form";
import { ModelForm } from "@/components/providers/model-form";
import { ModelPullDialog } from "@/components/providers/model-pull-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { useBatch } from "@/hooks/use-batch";
import { parseVariantNames } from "@/lib/variant-sync";

interface PriceTarget {
  model: ProviderModel;
  modelMatch: string;
}

function variantCount(v: unknown): number {
  if (Array.isArray(v)) return v.length;
  if (v && typeof v === "object" && Array.isArray((v as { suffixes?: unknown }).suffixes)) {
    return ((v as { suffixes: unknown[] }).suffixes).length;
  }
  return parseVariantNames(v).length;
}

function modelPriceTargets(model: ProviderModel): string[] {
  return [...new Set([model.model_id, ...parseVariantNames(model.variants_json)])];
}

export function ModelsTab({ provider }: { provider: Provider }) {
  const { t } = useTranslation("providers");
  const { t: tc } = useTranslation("common");
  const queryClient = useQueryClient();
  const { data: models, isPending } = useQuery(providerModelsQuery(provider.id));
  const { data: priceRules = [] } = useQuery(priceRulesQuery);
  const rows = models ?? [];
  const batch = useBatch("provider-models", ["providers", provider.id, "models"]);
  const ids = rows.map((m) => m.id);

  const [formOpen, setFormOpen] = useState(false);
  const [editTarget, setEditTarget] = useState<ProviderModel | undefined>(undefined);
  const [deleteTarget, setDeleteTarget] = useState<ProviderModel | undefined>(undefined);
  const [priceTarget, setPriceTarget] = useState<PriceTarget | undefined>(undefined);
  const [pullOpen, setPullOpen] = useState(false);
  const existingIds = new Set(rows.map((m) => m.model_id));

  const exactPriceRule = (modelMatch: string | undefined): PriceRule | undefined => {
    if (!modelMatch) return undefined;
    return priceRules.find((rule) =>
      rule.provider_id === provider.id &&
      rule.match_type === "exact" &&
      rule.model_match === modelMatch
    );
  };

  const removal = useMutation({
    mutationFn: (id: number) => deleteProviderModel(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["providers", provider.id, "models"] });
      setDeleteTarget(undefined);
    },
    onError: (error) => {
      toast.error(error instanceof ApiError ? error.message : String(error));
      setDeleteTarget(undefined);
    },
  });

  const actionsColumn = (m: ProviderModel) => (
    <div className="flex items-center justify-end gap-1">
      <Button variant="ghost" size="icon" aria-label={t("models.priceRule")} onClick={(e) => { e.stopPropagation(); setPriceTarget({ model: m, modelMatch: m.model_id }); }}>
        <BadgeDollarSign className="size-4" aria-hidden />
      </Button>
      <Button variant="ghost" size="icon" aria-label={t("models.edit")} onClick={(e) => { e.stopPropagation(); setEditTarget(m); setFormOpen(true); }}>
        <Pencil className="size-4" aria-hidden />
      </Button>
      <Button variant="ghost" size="icon" className="text-destructive" aria-label={t("models.delete")} onClick={(e) => { e.stopPropagation(); setDeleteTarget(m); }}>
        <Trash2 className="size-4" aria-hidden />
      </Button>
    </div>
  );

  const columns: DataColumn<ProviderModel>[] = [
    { key: "model", header: t("models.modelId"), cell: (m) => <span className="font-mono text-xs">{m.model_id}</span> },
    { key: "name", header: t("models.displayName"), cell: (m) => m.display_name ?? "—" },
    { key: "pricing", header: t("models.pricing"), cell: (m) => exactPriceRule(m.model_id) ? <Badge variant="secondary">{t("models.priced")}</Badge> : <span className="text-muted-foreground">—</span> },
    { key: "variants", header: t("models.variants"), cell: (m) => {
      const n = variantCount(m.variants_json);
      return n > 0 ? <Badge variant="outline">+{n}</Badge> : <span className="text-muted-foreground">—</span>;
    } },
    { key: "enabled", header: t("models.enabled"), cell: (m) => <Badge variant={m.enabled ? "secondary" : "outline"}>{m.enabled ? "on" : "off"}</Badge> },
    ...(batch.mode ? [] : [{ key: "actions", header: "", cell: actionsColumn, className: "w-28 text-right" } as DataColumn<ProviderModel>]),
  ];

  return (
    <div className="grid gap-3">
      <div className="flex items-center justify-end gap-2">
        {!batch.mode && (
          <>
            <Button variant="outline" onClick={() => setPullOpen(true)}>
              <DownloadCloud className="size-4" aria-hidden />{t("models.pull")}
            </Button>
            <Button onClick={() => { setEditTarget(undefined); setFormOpen(true); }}><Plus className="size-4" aria-hidden />{t("models.add")}</Button>
          </>
        )}
        <Button variant="outline" onClick={() => batch.mode ? batch.exit() : batch.setMode(true)}>
          {batch.mode ? tc("batch.cancel") : tc("batch.select")}
        </Button>
      </div>
      {isPending ? (
        <div className="grid gap-2" aria-busy="true"><Skeleton className="h-10" /></div>
      ) : (
        <DataTable
          columns={columns}
          rows={rows}
          rowKey={(m) => m.id}
          empty={t("models.empty")}
          selection={batch.mode ? {
            selectedIds: batch.selected,
            onToggle: batch.toggle,
            onToggleAll: () => batch.toggleAllFor(ids),
            allSelected: batch.allSelectedFor(ids),
            indeterminate: batch.selected.size > 0 && !batch.allSelectedFor(ids),
          } : undefined}
          renderCard={(m) => (
            <div className="grid gap-2">
              <div className="flex items-center justify-between">
                <span className="font-mono text-sm">{m.model_id}</span>
                <Badge variant={m.enabled ? "secondary" : "outline"}>{m.enabled ? "on" : "off"}</Badge>
              </div>
              <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                {m.display_name ? <span>{m.display_name}</span> : null}
                {exactPriceRule(m.model_id) ? <Badge variant="secondary">{t("models.priced")}</Badge> : null}
                {variantCount(m.variants_json) > 0 ? <Badge variant="outline">+{variantCount(m.variants_json)}</Badge> : null}
              </div>
              {!batch.mode && actionsColumn(m)}
            </div>
          )}
        />
      )}
      {batch.mode && (
        <BatchToolbar
          count={batch.selected.size}
          onEnable={batch.runEnable}
          onDisable={batch.runDisable}
          onDelete={batch.runDelete}
          onCancel={batch.exit}
          pending={batch.pending}
        />
      )}
      <EntityDialog open={formOpen} onOpenChange={setFormOpen} title={editTarget ? t("models.edit") : t("models.add")} wide>
        <ModelForm key={editTarget?.id ?? "new"} providerId={provider.id} providerName={provider.name} channel={provider.channel} model={editTarget} onSaved={() => setFormOpen(false)} />
      </EntityDialog>
      <EntityDialog open={priceTarget !== undefined} onOpenChange={(open) => !open && setPriceTarget(undefined)} title={t("models.priceRuleTitle", { name: priceTarget?.modelMatch ?? "" })} wide>
        {priceTarget && (
          <PriceRuleForm
            key={`${priceTarget.model.id}:${priceTarget.modelMatch}:${exactPriceRule(priceTarget.modelMatch)?.id ?? "new"}`}
            rule={exactPriceRule(priceTarget.modelMatch)}
            initialProviderId={provider.id}
            initialModelMatch={priceTarget.modelMatch}
            modelMatchOptions={modelPriceTargets(priceTarget.model)}
            onModelMatchChange={(modelMatch) => setPriceTarget({ model: priceTarget.model, modelMatch })}
            lockedTarget
            onSaved={() => setPriceTarget(undefined)}
          />
        )}
      </EntityDialog>
      <ModelPullDialog providerId={provider.id} existing={existingIds} open={pullOpen} onOpenChange={setPullOpen} />
      <ConfirmDangerous
        open={deleteTarget !== undefined}
        onOpenChange={(o) => { if (!o) setDeleteTarget(undefined); }}
        title={t("models.delete")}
        description={t("models.deleteConfirm", { name: deleteTarget?.model_id ?? "" })}
        confirmLabel={t("models.delete")}
        onConfirm={() => { if (deleteTarget) removal.mutate(deleteTarget.id); }}
        pending={removal.isPending}
      />
    </div>
  );
}

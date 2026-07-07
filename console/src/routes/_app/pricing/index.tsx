import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { DownloadCloud, Plus, Pencil, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { deletePriceRule, priceRulesQuery, type PriceRule } from "@/api/price-rules";
import { providersQuery } from "@/api/providers";
import { BatchToolbar } from "@/components/batch-toolbar";
import { DataTable, type DataColumn } from "@/components/data-table";
import { EntityDialog } from "@/components/entity-dialog";
import { DefaultPriceRuleImport } from "@/components/pricing/default-price-rule-import";
import { PriceRuleForm } from "@/components/pricing/price-rule-form";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { useBatch } from "@/hooks/use-batch";

export const Route = createFileRoute("/_app/pricing/")({
  loader: ({ context }) => context.queryClient.ensureQueryData(priceRulesQuery),
  component: PricingPage,
});

function PricingPage() {
  const { t } = useTranslation("pricing");
  const queryClient = useQueryClient();
  const { data: rules = [], isPending } = useQuery(priceRulesQuery);
  const { data: providers = [] } = useQuery(providersQuery);
  const [createOpen, setCreateOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [editing, setEditing] = useState<PriceRule | null>(null);
  const [deleting, setDeleting] = useState<number | null>(null);
  const batch = useBatch("price-rules", ["price-rules"]);

  const providerName = useMemo(
    () => new Map(providers.map((p) => [p.id, p.label ?? p.name])),
    [providers],
  );
  const rows = [...rules].sort((a, b) => a.id - b.id);

  const deleteMutation = useMutation({
    mutationFn: deletePriceRule,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["price-rules"] });
      toast.success(t("form.deleted"));
      setDeleting(null);
    },
  });

  const columns: DataColumn<PriceRule>[] = [
    {
      key: "scope",
      header: t("columns.scope"),
      cell: (r) => (
        <Badge variant={r.provider_id == null ? "outline" : "secondary"}>
          {r.provider_id == null ? t("scope.global") : providerName.get(r.provider_id) ?? `#${r.provider_id}`}
        </Badge>
      ),
    },
    {
      key: "match",
      header: t("columns.match"),
      cell: (r) => (
        <div className="grid gap-0.5">
          <span className="font-mono text-sm">{r.model_match}</span>
          <span className="text-xs text-muted-foreground">{t(`match.${r.match_type}`)}</span>
        </div>
      ),
    },
    {
      key: "input_price",
      header: t("columns.inputPrice"),
      cell: (r) => <span className="font-mono text-xs tabular-nums">{r.input_price}</span>,
    },
    {
      key: "output_price",
      header: t("columns.outputPrice"),
      cell: (r) => <span className="font-mono text-xs tabular-nums">{r.output_price}</span>,
    },
    {
      key: "cache_read_price",
      header: t("columns.cacheReadPrice"),
      cell: (r) => <span className="font-mono text-xs tabular-nums">{r.cache_read_price}</span>,
    },
    {
      key: "cache_creation_5m_price",
      header: t("columns.cacheCreation5mPrice"),
      cell: (r) => <span className="font-mono text-xs tabular-nums">{r.cache_creation_5m_price}</span>,
    },
    {
      key: "cache_creation_1h_price",
      header: t("columns.cacheCreation1hPrice"),
      cell: (r) => <span className="font-mono text-xs tabular-nums">{r.cache_creation_1h_price}</span>,
    },
    {
      key: "image_price",
      header: t("columns.imagePrice"),
      cell: (r) => <span className="font-mono text-xs tabular-nums">{r.image_price}</span>,
    },
    {
      key: "enabled",
      header: t("columns.status"),
      cell: (r) => <Badge variant={r.enabled ? "secondary" : "outline"}>{r.enabled ? t("status.enabled") : t("status.disabled")}</Badge>,
    },
    ...(batch.mode ? [] : [{
      key: "actions",
      header: "",
      cell: (r) => (
        <div className="flex justify-end gap-1">
          <Button size="icon" variant="ghost" onClick={(e) => { e.stopPropagation(); setEditing(r); }} aria-label={t("actions.edit")}>
            <Pencil className="size-4" aria-hidden />
          </Button>
          <Button size="icon" variant="ghost" onClick={(e) => { e.stopPropagation(); setDeleting(r.id); }} aria-label={t("actions.delete")}>
            <Trash2 className="size-4" aria-hidden />
          </Button>
        </div>
      ),
      className: "w-24 text-right",
    } as DataColumn<PriceRule>]),
  ];

  if (isPending) {
    return (
      <div className="grid gap-4 p-4 md:p-6" aria-busy="true">
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-4 w-72" />
        <Skeleton className="h-64" />
      </div>
    );
  }

  return (
    <div className="grid gap-4 p-4 md:p-6">
      <div className="flex items-center justify-between gap-3">
        <div>
          <h1 className="text-xl font-semibold">{t("title")}</h1>
          <p className="text-sm text-muted-foreground">{t("subtitle")}</p>
        </div>
        <div className="flex items-center gap-2">
          {!batch.mode && (
            <Button variant="outline" onClick={() => setImportOpen(true)}>
              <DownloadCloud className="size-4" aria-hidden />
              <span className="hidden sm:inline">{t("defaults.button")}</span>
            </Button>
          )}
          <Button variant="outline" onClick={() => batch.setMode(!batch.mode)}>
            {batch.mode ? t("batch.cancel", { ns: "common" }) : t("batch.select", { ns: "common" })}
          </Button>
          {!batch.mode && (
            <Button onClick={() => setCreateOpen(true)}>
              <Plus className="size-4" aria-hidden />
              <span className="hidden sm:inline">{t("add")}</span>
            </Button>
          )}
        </div>
      </div>

      <DataTable
        columns={columns}
        rows={rows}
        rowKey={(r) => r.id}
        empty={t("empty")}
        onRowClick={batch.mode ? undefined : (r) => setEditing(r)}
        selection={batch.mode ? {
          selectedIds: batch.selected,
          onToggle: batch.toggle,
          onToggleAll: () => batch.toggleAllFor(rows.map((r) => r.id)),
          allSelected: batch.allSelectedFor(rows.map((r) => r.id)),
          indeterminate: batch.selected.size > 0 && !batch.allSelectedFor(rows.map((r) => r.id)),
        } : undefined}
        renderCard={(r) => (
          <div className="grid gap-2">
            <div className="flex items-center justify-between gap-2">
              <span className="font-mono text-sm">{r.model_match}</span>
              <Badge variant={r.enabled ? "secondary" : "outline"}>{r.enabled ? t("status.enabled") : t("status.disabled")}</Badge>
            </div>
            <div className="text-xs text-muted-foreground">
              {r.provider_id == null ? t("scope.global") : providerName.get(r.provider_id) ?? `#${r.provider_id}`} · {t(`match.${r.match_type}`)}
            </div>
            <div className="grid grid-cols-2 gap-x-3 gap-y-1 font-mono text-xs text-muted-foreground">
              <span>{t("columns.inputPrice")}: {r.input_price}</span>
              <span>{t("columns.outputPrice")}: {r.output_price}</span>
              <span>{t("columns.cacheReadPrice")}: {r.cache_read_price}</span>
              <span>{t("columns.cacheCreation5mPrice")}: {r.cache_creation_5m_price}</span>
              <span>{t("columns.cacheCreation1hPrice")}: {r.cache_creation_1h_price}</span>
              <span>{t("columns.imagePrice")}: {r.image_price}</span>
            </div>
          </div>
        )}
      />

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

      <DefaultPriceRuleImport
        open={importOpen}
        onOpenChange={setImportOpen}
        existingRules={rules}
      />
      <EntityDialog open={createOpen} onOpenChange={setCreateOpen} title={t("add")} wide>
        <PriceRuleForm onSaved={() => setCreateOpen(false)} />
      </EntityDialog>
      <EntityDialog open={editing !== null} onOpenChange={(open) => !open && setEditing(null)} title={editing ? t("editTitle", { id: editing.id }) : ""} wide>
        {editing && <PriceRuleForm rule={editing} onSaved={() => setEditing(null)} />}
      </EntityDialog>
      <EntityDialog open={deleting !== null} onOpenChange={(open) => !open && setDeleting(null)} title={t("deleteTitle")}>
        <div className="grid gap-4">
          <p className="text-sm text-muted-foreground">{t("deleteConfirm")}</p>
          <div className="flex justify-end gap-2">
            <Button variant="outline" onClick={() => setDeleting(null)}>{t("actions.cancel", { ns: "common" })}</Button>
            <Button variant="destructive" onClick={() => deleting != null && deleteMutation.mutate(deleting)} disabled={deleteMutation.isPending}>
              {t("actions.delete", { ns: "common" })}
            </Button>
          </div>
        </div>
      </EntityDialog>
    </div>
  );
}

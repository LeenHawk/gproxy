import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { DownloadCloud, Pencil, Plus, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { deleteProviderModel, providerModelsQuery, type ProviderModel } from "@/api/provider-models";
import type { Provider } from "@/api/providers";
import { ApiError } from "@/api/http";
import { ConfirmDangerous } from "@/components/confirm-dangerous";
import { DataTable, type DataColumn } from "@/components/data-table";
import { EntityDialog } from "@/components/entity-dialog";
import { ModelForm } from "@/components/providers/model-form";
import { ModelPullDialog } from "@/components/providers/model-pull-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";

function variantCount(v: unknown): number {
  if (Array.isArray(v)) return v.length;
  if (v && typeof v === "object" && Array.isArray((v as { suffixes?: unknown }).suffixes)) {
    return ((v as { suffixes: unknown[] }).suffixes).length;
  }
  return 0;
}

export function ModelsTab({ provider }: { provider: Provider }) {
  const { t } = useTranslation("providers");
  const queryClient = useQueryClient();
  const { data: models, isPending } = useQuery(providerModelsQuery(provider.id));

  const [formOpen, setFormOpen] = useState(false);
  const [editTarget, setEditTarget] = useState<ProviderModel | undefined>(undefined);
  const [deleteTarget, setDeleteTarget] = useState<ProviderModel | undefined>(undefined);
  const [pullOpen, setPullOpen] = useState(false);
  const existingIds = new Set((models ?? []).map((m) => m.model_id));

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

  const actions = (m: ProviderModel) => (
    <div className="flex items-center justify-end gap-1">
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
    { key: "variants", header: t("models.variants"), cell: (m) => {
      const n = variantCount(m.variants_json);
      return n > 0 ? <Badge variant="outline">+{n}</Badge> : <span className="text-muted-foreground">—</span>;
    } },
    { key: "enabled", header: t("models.enabled"), cell: (m) => <Badge variant={m.enabled ? "secondary" : "outline"}>{m.enabled ? "on" : "off"}</Badge> },
    { key: "actions", header: "", cell: actions, className: "w-20 text-right" },
  ];

  return (
    <div className="grid gap-3">
      <div className="flex items-center justify-end gap-2">
        <Button variant="outline" onClick={() => setPullOpen(true)}>
          <DownloadCloud className="size-4" aria-hidden />{t("models.pull")}
        </Button>
        <Button onClick={() => { setEditTarget(undefined); setFormOpen(true); }}><Plus className="size-4" aria-hidden />{t("models.add")}</Button>
      </div>
      {isPending ? (
        <div className="grid gap-2" aria-busy="true"><Skeleton className="h-10" /></div>
      ) : (
        <DataTable
          columns={columns}
          rows={models ?? []}
          rowKey={(m) => m.id}
          empty={t("models.empty")}
          renderCard={(m) => (
            <div className="grid gap-2">
              <div className="flex items-center justify-between">
                <span className="font-mono text-sm">{m.model_id}</span>
                <Badge variant={m.enabled ? "secondary" : "outline"}>{m.enabled ? "on" : "off"}</Badge>
              </div>
              <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                {m.display_name ? <span>{m.display_name}</span> : null}
                {variantCount(m.variants_json) > 0 ? <Badge variant="outline">+{variantCount(m.variants_json)}</Badge> : null}
              </div>
              {actions(m)}
            </div>
          )}
        />
      )}
      <EntityDialog open={formOpen} onOpenChange={setFormOpen} title={editTarget ? t("models.edit") : t("models.add")} wide>
        <ModelForm key={editTarget?.id ?? "new"} providerId={provider.id} model={editTarget} onSaved={() => setFormOpen(false)} />
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

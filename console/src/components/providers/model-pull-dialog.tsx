import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Check, Loader2, Search } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { ApiError } from "@/api/http";
import { upstreamModelsQuery, upsertProviderModel } from "@/api/provider-models";
import { EntityDialog } from "@/components/entity-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";

/** Pull the provider's upstream model list and let the admin tick which to import
 *  as provider models (already-added ones are shown disabled). A search box
 *  filters the list — some upstreams (aggregators) offer hundreds of models. */
export function ModelPullDialog({
  providerId,
  existing,
  open,
  onOpenChange,
}: {
  providerId: number;
  existing: Set<string>;
  open: boolean;
  onOpenChange: (o: boolean) => void;
}) {
  const { t } = useTranslation("providers");
  const qc = useQueryClient();
  const q = useQuery(upstreamModelsQuery(providerId));
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [search, setSearch] = useState("");

  useEffect(() => {
    if (open) {
      setSelected(new Set());
      setSearch("");
      void q.refetch();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const models = q.data ?? [];
  const newModels = models.filter((m) => !existing.has(m.id));

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
  const visibleNew = visible.filter((m) => !existing.has(m.id));
  const allVisibleNewSelected =
    visibleNew.length > 0 && visibleNew.every((m) => selected.has(m.id));

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
      if (allVisibleNewSelected) visibleNew.forEach((m) => n.delete(m.id));
      else visibleNew.forEach((m) => n.add(m.id));
      return n;
    });

  const importMut = useMutation({
    mutationFn: async () => {
      const list = newModels.filter((m) => selected.has(m.id));
      for (const m of list) {
        await upsertProviderModel(providerId, {
          id: null,
          provider_id: providerId,
          model_id: m.id,
          display_name: m.display_name,
          pricing_json: null,
          enabled: true,
        });
      }
      return list.length;
    },
    onSuccess: (n) => {
      void qc.invalidateQueries({ queryKey: ["providers", providerId, "models"] });
      toast.success(t("models.imported", { count: n }));
      onOpenChange(false);
    },
    onError: (e) => toast.error(e instanceof ApiError ? e.message : String(e)),
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
                disabled={visibleNew.length === 0}
              >
                {allVisibleNewSelected ? t("models.selectNone") : t("models.selectAll")}
              </button>
              <span className="text-xs text-muted-foreground">
                {term
                  ? t("models.pullShown", { shown: visible.length, total: models.length })
                  : t("models.pullCount", { total: models.length, fresh: newModels.length })}
              </span>
            </div>
            <div className="max-h-[50vh] divide-y overflow-y-auto rounded-md border">
              {visible.length === 0 ? (
                <p className="py-6 text-center text-sm text-muted-foreground">
                  {t("models.pullNoMatch")}
                </p>
              ) : (
                visible.map((m) => {
                  const added = existing.has(m.id);
                  const sel = selected.has(m.id);
                  return (
                    <button
                      key={m.id}
                      type="button"
                      disabled={added}
                      onClick={() => toggle(m.id)}
                      className={cn(
                        "flex w-full items-center gap-3 px-3 py-2 text-left text-sm disabled:opacity-60",
                        !added && "hover:bg-accent/50",
                        sel && "bg-primary/5",
                      )}
                    >
                      <span
                        className={cn(
                          "grid size-4 shrink-0 place-items-center rounded border",
                          sel
                            ? "border-primary bg-primary text-primary-foreground"
                            : "border-input",
                        )}
                      >
                        {sel && <Check className="size-3" aria-hidden />}
                      </span>
                      <span className="flex-1 truncate font-mono text-xs">{m.id}</span>
                      {m.display_name && (
                        <span className="truncate text-xs text-muted-foreground">
                          {m.display_name}
                        </span>
                      )}
                      {added && (
                        <Badge variant="outline" className="text-[10px]">
                          {t("models.alreadyAdded")}
                        </Badge>
                      )}
                    </button>
                  );
                })
              )}
            </div>
            <Button
              disabled={selected.size === 0 || importMut.isPending}
              onClick={() => importMut.mutate()}
            >
              {importMut.isPending && <Loader2 className="mr-2 size-4 animate-spin" aria-hidden />}
              {t("models.import", { count: selected.size })}
            </Button>
          </>
        )}
      </div>
    </EntityDialog>
  );
}

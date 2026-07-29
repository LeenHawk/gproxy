import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { Plus } from "lucide-react";
import { useTranslation } from "react-i18next";
import { providersQuery, type Provider } from "@/api/providers";
import { channelMeta, type ChannelMeta } from "@/lib/channel-meta";
import { DataTable, type DataColumn } from "@/components/data-table";
import { EntityDialog } from "@/components/entity-dialog";
import { ProviderForm } from "@/components/providers/provider-form";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { useBatch } from "@/hooks/use-batch";
import { useChannelCatalog } from "@/hooks/use-channel-catalog";
import { BatchToolbar } from "@/components/batch-toolbar";

export const Route = createFileRoute("/_app/providers/")({
  loader: ({ context }) => context.queryClient.ensureQueryData(providersQuery),
  component: ProvidersPage,
});

function EnabledBadge({ enabled }: { enabled: boolean }) {
  return <Badge variant={enabled ? "secondary" : "outline"}>{enabled ? "on" : "off"}</Badge>;
}

function ChannelName({ id, meta }: { id: string; meta?: ChannelMeta }) {
  const displayName = meta?.displayName ?? id;
  return (
    <span className="flex flex-wrap items-baseline gap-x-2">
      <span>{displayName}</span>
      {displayName !== id && <span className="font-mono text-xs text-muted-foreground">{id}</span>}
    </span>
  );
}

function ProvidersPage() {
  const { t } = useTranslation("providers");
  const navigate = useNavigate();
  const { data: providers, isPending } = useQuery(providersQuery);
  const catalogState = useChannelCatalog();
  const catalog = catalogState.catalog;
  const [createOpen, setCreateOpen] = useState(false);

  const rows = providers ?? [];
  const batch = useBatch("providers", ["providers"]);
  const ids = rows.map((p) => p.id);

  const columns: DataColumn<Provider>[] = [
    { key: "name", header: t("fields.name"), cell: (p) => (
      <span className="font-medium">{p.label ?? p.name}</span>
    ) },
    { key: "channel", header: t("fields.channel"), cell: (p) => (
      <ChannelName id={p.channel} meta={channelMeta(p.channel, catalog)} />
    ) },
    { key: "family", header: "", cell: (p) => {
      const meta = channelMeta(p.channel, catalog);
      return meta ? <Badge variant="outline">{t(`family.${meta.family}`)}</Badge> : null;
    } },
    { key: "strategy", header: t("fields.strategy"), cell: (p) => t(`strategy.${p.credential_strategy}`, { defaultValue: p.credential_strategy }) },
    { key: "enabled", header: t("fields.enabled"), cell: (p) => <EnabledBadge enabled={p.enabled} /> },
  ];

  return (
    <div className="grid gap-4 p-4 md:p-6">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold">{t("title")}</h1>
        <div className="flex items-center gap-2">
          <Button variant="outline" onClick={() => batch.setMode(!batch.mode)}>
            {batch.mode ? t("batch.cancel", { ns: "common" }) : t("batch.select", { ns: "common" })}
          </Button>
          <Button
            disabled={!catalogState.authoritative || catalog.length === 0}
            onClick={() => {
              if (!catalogState.authoritative || catalog.length === 0) return;
              setCreateOpen(true);
            }}
          >
            <Plus className="size-4" aria-hidden />
            <span className="hidden sm:inline">{t("new")}</span>
          </Button>
        </div>
      </div>

      {catalogState.availability !== "ready" && (
        <div className="flex flex-wrap items-center justify-between gap-2 rounded-md border px-3 py-2 text-sm">
          <span className={catalogState.availability === "error" ? "text-destructive" : "text-muted-foreground"}>
            {t(`catalog.${catalogState.availability}`)}
            {catalogState.availability === "error" && catalogState.error instanceof Error
              ? ` ${catalogState.error.message}`
              : ""}
          </span>
          {catalogState.availability !== "loading" && (
            <Button
              size="sm"
              variant="outline"
              disabled={catalogState.isFetching}
              onClick={() => { void catalogState.refetch(); }}
            >
              {t("catalog.retry")}
            </Button>
          )}
        </div>
      )}

      {catalogState.availability === "ready" && catalog.length === 0 && (
        <p className="text-sm text-muted-foreground">{t("catalog.empty")}</p>
      )}

      {isPending ? (
        <div className="grid gap-2" aria-busy="true">
          <Skeleton className="h-10" /><Skeleton className="h-10" /><Skeleton className="h-10" />
        </div>
      ) : (
        <DataTable
          columns={columns}
          rows={rows}
          rowKey={(p) => p.id}
          empty={t("empty")}
          onRowClick={batch.mode ? undefined : (p) => void navigate({ to: "/providers/$providerId", params: { providerId: String(p.id) } })}
          selection={batch.mode ? {
            selectedIds: batch.selected,
            onToggle: batch.toggle,
            onToggleAll: () => batch.toggleAllFor(ids),
            allSelected: batch.allSelectedFor(ids),
            indeterminate: batch.selected.size > 0 && !batch.allSelectedFor(ids),
          } : undefined}
          renderCard={(p) => {
            const meta = channelMeta(p.channel, catalog);
            return (
              <div className="grid gap-1">
                <div className="flex items-center justify-between">
                  <span className="font-medium">{p.label ?? p.name}</span>
                  <EnabledBadge enabled={p.enabled} />
                </div>
                <div className="flex items-center gap-2 text-xs text-muted-foreground">
                  <ChannelName id={p.channel} meta={meta} />
                  {meta && <Badge variant="outline">{t(`family.${meta.family}`)}</Badge>}
                  <span>·</span>
                  <span>{t(`strategy.${p.credential_strategy}`, { defaultValue: p.credential_strategy })}</span>
                </div>
              </div>
            );
          }}
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

      <EntityDialog open={createOpen} onOpenChange={setCreateOpen} title={t("new")} wide>
        <ProviderForm
          onSaved={(saved) => {
            setCreateOpen(false);
            void navigate({ to: "/providers/$providerId", params: { providerId: String(saved.id) } });
          }}
        />
      </EntityDialog>
    </div>
  );
}

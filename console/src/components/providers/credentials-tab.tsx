import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Gauge, KeyRound, Pencil, Plus, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { credentialsQuery, deleteCredential, type CredentialView } from "@/api/credentials";
import type { Provider } from "@/api/providers";
import { ApiError } from "@/api/http";
import { BatchToolbar } from "@/components/batch-toolbar";
import { ConfirmDangerous } from "@/components/confirm-dangerous";
import { DataTable, type DataColumn } from "@/components/data-table";
import { EntityDialog } from "@/components/entity-dialog";
import { CredentialBulkImport } from "@/components/providers/credential-bulk-import";
import { CredentialForm } from "@/components/providers/credential-form";
import { HealthBadge } from "@/components/providers/health-badge";
import { OAuthWizard } from "@/components/providers/oauth-wizard";
import { UsageCard } from "@/components/providers/usage-card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { useBatch } from "@/hooks/use-batch";
import { useChannelMeta } from "@/hooks/use-channel-catalog";

function credName(c: CredentialView, fallback: string): string {
  return c.label ?? fallback;
}

export function CredentialsTab({ provider }: { provider: Provider }) {
  const { t } = useTranslation("providers");
  const { t: tc } = useTranslation("common");
  const queryClient = useQueryClient();
  const { data: creds, isPending } = useQuery(credentialsQuery(provider.id));
  const catalogState = useChannelMeta(provider.channel);
  const { meta } = catalogState;
  const canMutate = catalogState.authoritative && meta !== undefined;
  const catalogMessageKey = catalogState.availability === "ready"
    ? "catalog.metadataUnavailable"
    : `catalog.${catalogState.availability}`;
  const rows = creds ?? [];
  const batch = useBatch("credentials", ["providers", provider.id, "credentials"]);
  const ids = rows.map((c) => c.id);

  const [formOpen, setFormOpen] = useState(false);
  const [editTarget, setEditTarget] = useState<CredentialView | undefined>(undefined);
  const [deleteTarget, setDeleteTarget] = useState<CredentialView | undefined>(undefined);
  const [wizardOpen, setWizardOpen] = useState(false);
  const [usageTarget, setUsageTarget] = useState<CredentialView | undefined>(undefined);
  const [bulkOpen, setBulkOpen] = useState(false);

  const removal = useMutation({
    mutationFn: (id: number) => deleteCredential(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["providers", provider.id, "credentials"] });
      setDeleteTarget(undefined);
    },
    onError: (error) => {
      toast.error(error instanceof ApiError ? error.message : String(error));
      setDeleteTarget(undefined); // close on error (toast carries the reason) — uniform with F2 delete flows
    },
  });

  const openCreate = () => {
    if (!canMutate) return;
    setEditTarget(undefined);
    setFormOpen(true);
  };
  const openEdit = (c: CredentialView) => {
    if (!canMutate) return;
    setEditTarget(c);
    setFormOpen(true);
  };

  const actions = (c: CredentialView) => (
    <div className="flex items-center justify-end gap-1">
      {meta?.usage && (
        <Button variant="ghost" size="icon" aria-label={t("usage.open")} onClick={(e) => { e.stopPropagation(); setUsageTarget(c); }}>
          <Gauge className="size-4" aria-hidden />
        </Button>
      )}
      <Button
        variant="ghost"
        size="icon"
        aria-label={t("creds.edit")}
        disabled={!canMutate}
        onClick={(e) => { e.stopPropagation(); openEdit(c); }}
      >
        <Pencil className="size-4" aria-hidden />
      </Button>
      <Button variant="ghost" size="icon" className="text-destructive" aria-label={t("delete.credential")}
        onClick={(e) => { e.stopPropagation(); setDeleteTarget(c); }}>
        <Trash2 className="size-4" aria-hidden />
      </Button>
    </div>
  );

  const columns: DataColumn<CredentialView>[] = [
    { key: "label", header: t("fields.credLabel"), cell: (c) => (
      <span className="font-medium">{credName(c, t("creds.unnamed", { id: c.id }))}</span>
    ) },
    { key: "kind", header: t("fields.kind"), cell: (c) => <span className="font-mono text-xs">{c.kind}</span> },
    { key: "weight", header: t("fields.weight"), cell: (c) => c.weight },
    { key: "limits", header: `${t("fields.rpm")}/${t("fields.tpm")}`, cell: (c) => (
      <span className="text-xs text-muted-foreground">{c.rpm_limit ?? "—"} / {c.tpm_limit ?? "—"}</span>
    ) },
    { key: "secret", header: "", cell: (c) => (
      <Badge variant={c.has_secret ? "secondary" : "outline"}>
        <KeyRound className="size-3" />
        {c.has_secret ? t("creds.hasSecret") : t("creds.noSecret")}
      </Badge>
    ) },
    { key: "enabled", header: t("fields.enabled"), cell: (c) => (
      <Badge variant={c.enabled ? "secondary" : "outline"}>{c.enabled ? "on" : "off"}</Badge>
    ) },
    { key: "health", header: t("health.title"), cell: (c) => <HealthBadge credentialId={c.id} /> },
    ...(batch.mode ? [] : [{ key: "actions", header: "", cell: actions, className: "w-24 text-right" } as DataColumn<CredentialView>]),
  ];

  return (
    <div className="grid gap-3">
      <div className="flex items-center justify-end gap-2">
        {!batch.mode && (
          <>
            {(meta?.loginModes.length ?? 0) > 0 && (
              <Button
                variant="outline"
                disabled={!canMutate}
                onClick={() => {
                  if (!canMutate) return;
                  setWizardOpen(true);
                }}
              >
                {t("creds.oauth")}
              </Button>
            )}
            {meta?.family === "api_key" && (
              <Button
                variant="outline"
                disabled={!canMutate}
                onClick={() => {
                  if (!canMutate) return;
                  setBulkOpen(true);
                }}
              >
                {t("creds.bulk.button")}
              </Button>
            )}
            <Button disabled={!canMutate} onClick={openCreate}>
              <Plus className="size-4" aria-hidden />
              {t("creds.manual")}
            </Button>
          </>
        )}
        <Button variant="outline" onClick={() => batch.mode ? batch.exit() : batch.setMode(true)}>
          {batch.mode ? tc("batch.cancel") : tc("batch.select")}
        </Button>
      </div>

      {!canMutate && (
        <div className="flex flex-wrap items-center justify-between gap-2 rounded-md border px-3 py-2 text-sm">
          <span className={catalogState.availability === "error" ? "text-destructive" : "text-muted-foreground"}>
            {t(catalogMessageKey)}
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

      {isPending ? (
        <div className="grid gap-2" aria-busy="true"><Skeleton className="h-10" /><Skeleton className="h-10" /></div>
      ) : (
        <DataTable
          columns={columns}
          rows={rows}
          rowKey={(c) => c.id}
          empty={t("creds.empty")}
          selection={batch.mode ? {
            selectedIds: batch.selected,
            onToggle: batch.toggle,
            onToggleAll: () => batch.toggleAllFor(ids),
            allSelected: batch.allSelectedFor(ids),
            indeterminate: batch.selected.size > 0 && !batch.allSelectedFor(ids),
          } : undefined}
          renderCard={(c) => (
            <div className="grid gap-2">
              <div className="flex items-center justify-between">
                <span className="font-medium">{credName(c, t("creds.unnamed", { id: c.id }))}</span>
                <Badge variant={c.enabled ? "secondary" : "outline"}>{c.enabled ? "on" : "off"}</Badge>
              </div>
              <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                <span className="font-mono">{c.kind}</span>
                <span>w{c.weight}</span>
                <span>{c.rpm_limit ?? "—"}/{c.tpm_limit ?? "—"}</span>
                <Badge variant={c.has_secret ? "secondary" : "outline"}>
                  {c.has_secret ? t("creds.hasSecret") : t("creds.noSecret")}
                </Badge>
                <HealthBadge credentialId={c.id} />
              </div>
              {!batch.mode && actions(c)}
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

      <EntityDialog
        open={formOpen}
        onOpenChange={setFormOpen}
        title={editTarget ? t("creds.edit") : t("creds.manual")}
        description={meta ? t(`family.${meta.family}`) : undefined}
        wide
      >
        {canMutate && meta && (
          <CredentialForm
            key={editTarget?.id ?? "new"}
            providerId={provider.id}
            meta={meta}
            metadataAuthoritative={catalogState.authoritative}
            credential={editTarget}
            onSaved={() => setFormOpen(false)}
          />
        )}
      </EntityDialog>

      <ConfirmDangerous
        open={deleteTarget !== undefined}
        onOpenChange={(o) => { if (!o) setDeleteTarget(undefined); }}
        title={t("delete.credential")}
        description={t("delete.credentialConfirm", {
          name: deleteTarget ? credName(deleteTarget, t("creds.unnamed", { id: deleteTarget.id })) : "",
        })}
        confirmLabel={t("delete.credential")}
        onConfirm={() => { if (deleteTarget) removal.mutate(deleteTarget.id); }}
        pending={removal.isPending}
      />

      <EntityDialog
        open={wizardOpen}
        onOpenChange={setWizardOpen}
        title={t("wizard.title", { channel: meta?.displayName ?? provider.channel })}
      >
        {canMutate && meta && (
          <OAuthWizard
            provider={provider}
            meta={meta}
            onDone={() => {
              toast.success(t("wizard.done"));
              setWizardOpen(false);
            }}
          />
        )}
      </EntityDialog>

      <EntityDialog
        open={usageTarget !== undefined}
        onOpenChange={(o) => { if (!o) setUsageTarget(undefined); }}
        title={`${t("usage.title")} — ${usageTarget ? credName(usageTarget, t("creds.unnamed", { id: usageTarget.id })) : ""}`}
      >
        {usageTarget && <UsageCard credentialId={usageTarget.id} />}
      </EntityDialog>

      <EntityDialog
        open={bulkOpen}
        onOpenChange={setBulkOpen}
        title={t("creds.bulk.title")}
        description={t("creds.bulk.textareaHint")}
        wide
      >
        {canMutate && (
          <CredentialBulkImport
            key={bulkOpen ? "open" : "closed"}
            providerId={provider.id}
            metadataAuthoritative={catalogState.authoritative}
            onClose={() => setBulkOpen(false)}
          />
        )}
      </EntityDialog>
    </div>
  );
}

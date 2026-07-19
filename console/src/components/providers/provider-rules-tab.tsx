import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Plus } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  providerRuleSetsQuery,
  ruleSetsQuery,
  upsertRuleSet,
  upsertProviderRuleSet,
  deleteProviderRuleSet,
  type ProviderRuleSet,
} from "@/api/rules";
import { ApiError } from "@/api/http";
import { BatchToolbar } from "@/components/batch-toolbar";
import { ConfirmDangerous } from "@/components/confirm-dangerous";
import { DataTable } from "@/components/data-table";
import { EntityDialog } from "@/components/entity-dialog";
import { ProviderRuleSetForm } from "@/components/rules/provider-rule-set-form";
import { RulePipeline } from "@/components/rules/rule-pipeline";
import { RuleSetDrawer } from "@/components/rules/rule-set-drawer";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { useBatch } from "@/hooks/use-batch";
import {
  buildProviderRuleSetColumns,
  ProviderRuleSetCard,
} from "./provider-rule-set-row";
import { OpenCodeRuleSetButton } from "./opencode-rule-set-button";

export function ProviderRulesTab({ providerId }: { providerId: number }) {
  const { t } = useTranslation("rules");
  const { t: tc } = useTranslation("common");
  const qc = useQueryClient();
  const { data: attachments = [], isPending } = useQuery(providerRuleSetsQuery(providerId));
  const { data: allRuleSets = [] } = useQuery(ruleSetsQuery);
  const batch = useBatch("provider-rule-sets", ["providers", providerId, "rule-sets"]);
  const ids = attachments.map((a) => a.id);

  const [attachOpen, setAttachOpen] = useState(false);
  const [editAttach, setEditAttach] = useState<ProviderRuleSet | undefined>(undefined);
  const [delAttach, setDelAttach] = useState<ProviderRuleSet | undefined>(undefined);
  const [drawerSetId, setDrawerSetId] = useState<number | null>(null);

  const rsName = new Map(allRuleSets.map((rs) => [rs.id, rs.name]));
  const attachedIds = attachments.map((a) => a.rule_set_id);

  const invalidate = () => {
    void qc.invalidateQueries({ queryKey: ["providers", providerId, "rule-sets"] });
    void qc.invalidateQueries({ queryKey: ["rule-sets"] });
  };

  const createAndAttach = useMutation({
    mutationFn: async () => {
      const rs = await upsertRuleSet({
        name: t("createAndAttach") + " " + new Date().toISOString().slice(0, 19),
        enabled: true,
      });
      await upsertProviderRuleSet(providerId, {
        provider_id: providerId,
        rule_set_id: rs.id,
        sort_order: attachments.length,
        enabled: true,
      });
      return rs;
    },
    onSuccess: (rs) => { invalidate(); setDrawerSetId(rs.id); },
    onError: (e) => toast.error(e instanceof ApiError ? e.message : String(e)),
  });

  const removal = useMutation({
    mutationFn: (id: number) => deleteProviderRuleSet(id),
    onSuccess: () => { invalidate(); setDelAttach(undefined); },
    onError: (e) => {
      toast.error(e instanceof ApiError ? e.message : String(e));
      setDelAttach(undefined);
    },
  });

  const columns = batch.mode
    ? buildProviderRuleSetColumns(t, rsName, () => {}, () => {}, () => {}).filter((c) => c.key !== "actions")
    : buildProviderRuleSetColumns(
        t,
        rsName,
        (id) => setDrawerSetId(id),
        (a) => { setEditAttach(a); setAttachOpen(true); },
        (a) => setDelAttach(a),
      );

  return (
    <div className="grid gap-4">
      <RulePipeline providerId={providerId} />

      <div className="flex items-center justify-between gap-2">
        <h3 className="text-sm font-medium">{t("providerRuleSet.ruleSet")}</h3>
        <div className="flex gap-2">
          {!batch.mode && (
            <>
              <OpenCodeRuleSetButton
                providerId={providerId}
                ruleSets={allRuleSets}
                attachments={attachments}
                onApplied={(id) => { invalidate(); setDrawerSetId(id); }}
              />
              <Button
                variant="outline"
                size="sm"
                onClick={() => { setEditAttach(undefined); setAttachOpen(true); }}
              >
                <Plus className="size-4" />
                {t("attachExisting")}
              </Button>
              <Button
                size="sm"
                onClick={() => createAndAttach.mutate()}
                disabled={createAndAttach.isPending}
              >
                <Plus className="size-4" />
                {t("createAndAttach")}
              </Button>
            </>
          )}
          <Button variant="outline" size="sm" onClick={() => batch.mode ? batch.exit() : batch.setMode(true)}>
            {batch.mode ? tc("batch.cancel") : tc("batch.select")}
          </Button>
        </div>
      </div>

      {isPending ? (
        <Skeleton className="h-10" />
      ) : (
        <DataTable
          columns={columns}
          rows={attachments}
          rowKey={(a) => a.id}
          empty={t("providerRuleSet.empty")}
          onRowClick={batch.mode ? undefined : (a) => setDrawerSetId(a.rule_set_id)}
          selection={batch.mode ? {
            selectedIds: batch.selected,
            onToggle: batch.toggle,
            onToggleAll: () => batch.toggleAllFor(ids),
            allSelected: batch.allSelectedFor(ids),
            indeterminate: batch.selected.size > 0 && !batch.allSelectedFor(ids),
          } : undefined}
          renderCard={(a) => (
            <ProviderRuleSetCard
              a={a}
              rsName={rsName}
              onEdit={(id) => setDrawerSetId(id)}
              onAttach={(a) => { setEditAttach(a); setAttachOpen(true); }}
              onDetach={(a) => setDelAttach(a)}
              batchMode={batch.mode}
            />
          )}
        />
      )}

      <EntityDialog
        open={attachOpen}
        onOpenChange={setAttachOpen}
        title={editAttach ? t("providerRuleSet.ruleSet") : t("attachExisting")}
      >
        <ProviderRuleSetForm
          key={editAttach?.id ?? "new"}
          providerId={providerId}
          attachment={editAttach}
          ruleSets={allRuleSets}
          attachedIds={attachedIds}
          onSaved={() => { setAttachOpen(false); invalidate(); }}
        />
      </EntityDialog>

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

      <RuleSetDrawer
        ruleSetId={drawerSetId}
        providerId={providerId}
        open={drawerSetId !== null}
        onOpenChange={(o) => { if (!o) { setDrawerSetId(null); invalidate(); } }}
      />

      <ConfirmDangerous
        open={delAttach !== undefined}
        onOpenChange={(o) => { if (!o) setDelAttach(undefined); }}
        title={t("providerRuleSet.unattach")}
        description={t("providerRuleSet.unattachConfirm")}
        confirmLabel={t("providerRuleSet.unattach")}
        onConfirm={() => delAttach && removal.mutate(delAttach.id)}
        pending={removal.isPending}
      />
    </div>
  );
}

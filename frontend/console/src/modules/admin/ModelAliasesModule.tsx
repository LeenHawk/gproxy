import { useEffect, useMemo, useState } from "react";

import { useI18n } from "../../app/i18n";
import { Button, Card, Input, Label, Select } from "../../components/ui";
import { apiJson, apiVoid } from "../../lib/api";
import { authHeaders } from "../../lib/auth";
import { parseRequiredI64 } from "../../lib/form";
import type { MemoryModelRow, ModelWrite, ProviderRow } from "../../lib/types/admin";

type PullModelsResponse = { models: string[] };

/**
 * Derive a legacy-shaped alias row from the unified MemoryModelRow[].
 * alias_model is the alias row (alias_of != null), allModels includes real models.
 */
function deriveAliasDisplay(
  alias: MemoryModelRow,
  allModels: MemoryModelRow[],
  providers: ProviderRow[],
): { id: number; aliasName: string; providerName: string; targetModelId: string; providerId: number } | null {
  if (alias.alias_of == null) return null;
  const target = allModels.find((m) => m.id === alias.alias_of);
  if (!target) return null;
  const provider = providers.find((p) => p.id === target.provider_id);
  return {
    id: alias.id,
    aliasName: alias.model_id,
    providerName: provider?.name ?? String(target.provider_id),
    targetModelId: target.model_id,
    providerId: target.provider_id,
  };
}

export function ModelAliasesModule({
  sessionToken,
  notify,
}: {
  sessionToken: string;
  notify: (kind: "success" | "error" | "info", message: string) => void;
}) {
  const { t } = useI18n();
  const headers = useMemo(() => authHeaders(sessionToken), [sessionToken]);
  const [providers, setProviders] = useState<ProviderRow[]>([]);
  const [allModels, setAllModels] = useState<MemoryModelRow[]>([]);
  const [selectedAliasName, setSelectedAliasName] = useState<string | null>(null);
  const [form, setForm] = useState({
    id: "",
    alias: "",
    provider_id: "",
    model_id: "",
    enabled: true,
  });

  const aliasRows = useMemo(() => allModels.filter((m) => m.alias_of != null), [allModels]);
  const aliasDisplayRows = useMemo(
    () => aliasRows.map((a) => deriveAliasDisplay(a, allModels, providers)).filter(Boolean) as NonNullable<ReturnType<typeof deriveAliasDisplay>>[],
    [aliasRows, allModels, providers],
  );

  const nextId = useMemo(
    () => allModels.reduce((max, row) => Math.max(max, row.id), 0) + 1,
    [allModels],
  );

  // Pull modal state
  const [pullOpen, setPullOpen] = useState(false);
  const [pullProviderId, setPullProviderId] = useState("");
  const [pullLoading, setPullLoading] = useState(false);
  const [pulledModels, setPulledModels] = useState<string[] | null>(null);
  const [pullSelected, setPullSelected] = useState<Set<string>>(new Set());

  const beginCreate = () => {
    setSelectedAliasName(null);
    setForm({
      id: String(nextId),
      alias: "",
      provider_id: providers[0] ? String(providers[0].id) : "",
      model_id: "",
      enabled: true,
    });
  };

  const load = async () => {
    const [providerRows, modelRows] = await Promise.all([
      apiJson<ProviderRow[]>("/admin/providers/query", {
        method: "POST",
        headers,
        body: JSON.stringify({}),
      }),
      apiJson<MemoryModelRow[]>("/admin/models/query", {
        method: "POST",
        headers,
        body: JSON.stringify({}),
      }),
    ]);
    setProviders(providerRows);
    setAllModels(modelRows);
  };

  useEffect(() => {
    void load().catch((error) => notify("error", error instanceof Error ? error.message : String(error)));
  }, []);

  useEffect(() => {
    if (!selectedAliasName && !form.id && providers.length > 0) {
      beginCreate();
    }
  }, [form.id, providers, selectedAliasName]);

  const save = async () => {
    try {
      const providerId = parseRequiredI64(form.provider_id, "provider_id");
      const targetModelId = form.model_id.trim();
      // Find the target real model to get its id for alias_of
      const target = allModels.find(
        (m) => m.provider_id === providerId && m.model_id === targetModelId && m.alias_of == null,
      );
      const aliasOf = target?.id ?? null;

      const payload: ModelWrite = {
        id: parseRequiredI64(form.id, "id"),
        provider_id: providerId,
        model_id: form.alias.trim(),
        display_name: null,
        enabled: form.enabled,
        price_each_call: null,
        price_tiers_json: null,
        alias_of: aliasOf,
      };
      await apiJson("/admin/models/upsert", {
        method: "POST",
        headers,
        body: JSON.stringify(payload),
      });
      notify("success", t("modelAliases.saved"));
      await load();
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  const remove = async (aliasName: string) => {
    try {
      const aliasModel = aliasRows.find((m) => m.model_id === aliasName);
      if (!aliasModel) {
        notify("error", "Alias not found");
        return;
      }
      await apiVoid("/admin/models/delete", {
        method: "POST",
        headers,
        body: JSON.stringify({ id: aliasModel.id }),
      });
      notify("success", t("modelAliases.deleted"));
      await load();
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  const openPull = () => {
    setPullOpen(true);
    setPulledModels(null);
    setPullSelected(new Set());
    setPullProviderId(providers[0] ? String(providers[0].id) : "");
  };

  const closePull = () => {
    setPullOpen(false);
    setPulledModels(null);
    setPullSelected(new Set());
  };

  const doPull = async () => {
    if (!pullProviderId) return;
    setPullLoading(true);
    try {
      const resp = await apiJson<PullModelsResponse>("/admin/models/pull", {
        method: "POST",
        headers,
        body: JSON.stringify({ provider_id: Number(pullProviderId) }),
      });

      // Filter out models that already exist as aliases
      const existingAliases = new Set(aliasRows.map((row) => row.model_id));
      const newModels = resp.models.filter((m) => !existingAliases.has(m));

      setPulledModels(newModels);
      setPullSelected(new Set(newModels));
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    } finally {
      setPullLoading(false);
    }
  };

  const importSelected = async () => {
    if (pullSelected.size === 0) return;
    try {
      const maxId = allModels.reduce((max, row) => Math.max(max, row.id), 0);
      const providerId = Number(pullProviderId);
      const items: ModelWrite[] = [...pullSelected].map((model, index) => {
        // Find target real model for alias_of
        const target = allModels.find(
          (m) => m.provider_id === providerId && m.model_id === model && m.alias_of == null,
        );
        return {
          id: maxId + index + 1,
          provider_id: providerId,
          model_id: model,
          display_name: null,
          enabled: true,
          price_each_call: null,
          price_tiers_json: null,
          alias_of: target?.id ?? null,
        };
      });
      await apiJson("/admin/models/batch-upsert", {
        method: "POST",
        headers,
        body: JSON.stringify(items),
      });
      notify("success", t("modelAliases.pull.imported", { count: items.length }));
      closePull();
      await load();
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <Card title={t("modelAliases.title")}>
      <div className="grid gap-4 xl:grid-cols-[360px_minmax(0,1fr)]">
        <div className="space-y-2">
          <div className="flex gap-2 justify-end">
            <Button variant="neutral" onClick={openPull}>{t("modelAliases.pull")}</Button>
          </div>
          {aliasDisplayRows.map((row) => (
            <div
              key={row.id}
              className={`card-shell cursor-pointer ${row.aliasName === selectedAliasName ? "nav-item-active" : ""}`}
              onClick={() => {
                setSelectedAliasName(row.aliasName);
                setForm({
                  id: String(row.id),
                  alias: row.aliasName,
                  provider_id: String(row.providerId),
                  model_id: row.targetModelId,
                  enabled: true,
                });
              }}
            >
              <div className="font-semibold">{row.aliasName}</div>
              <div className="text-xs text-muted">{row.providerName} / {row.targetModelId}</div>
            </div>
          ))}
        </div>
        <div className="card-shell space-y-3">
          <div className="flex justify-end">
            <Button variant="neutral" onClick={beginCreate}>{t("common.create")}</Button>
          </div>
          <div>
            <Label>{t("common.provider")}</Label>
            <Select
              value={form.provider_id}
              onChange={(value) => setForm((current) => ({ ...current, provider_id: value }))}
              options={providers.map((provider) => ({ value: String(provider.id), label: provider.name }))}
            />
          </div>
          <div>
            <Label>alias</Label>
            <Input value={form.alias} onChange={(value) => setForm((current) => ({ ...current, alias: value }))} />
          </div>
          <div>
            <Label>{t("common.modelId")}</Label>
            <Input value={form.model_id} onChange={(value) => setForm((current) => ({ ...current, model_id: value }))} />
          </div>
          <div className="flex gap-2">
            <Button onClick={() => void save()}>{t("common.save")}</Button>
            {selectedAliasName ? <Button variant="danger" onClick={() => void remove(selectedAliasName)}>{t("common.delete")}</Button> : null}
          </div>
        </div>
      </div>

      {/* Pull Models Modal */}
      {pullOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onClick={closePull}>
          <div className="card-shell w-full max-w-lg max-h-[80vh] overflow-y-auto p-6 space-y-4" onClick={(e) => e.stopPropagation()}>
            <h3 className="text-lg font-semibold">{t("modelAliases.pull")}</h3>
            <div>
              <Label>{t("modelAliases.pull.selectProvider")}</Label>
              <Select
                value={pullProviderId}
                onChange={setPullProviderId}
                options={providers.map((provider) => ({ value: String(provider.id), label: provider.name }))}
              />
            </div>
            {pulledModels === null ? (
              <div className="flex gap-2 justify-end">
                <Button variant="neutral" onClick={closePull}>{t("common.cancel")}</Button>
                <Button onClick={() => void doPull()} disabled={pullLoading || !pullProviderId}>
                  {pullLoading ? t("modelAliases.pull.loading") : t("common.fetch")}
                </Button>
              </div>
            ) : pulledModels.length === 0 ? (
              <div className="space-y-3">
                <p className="text-muted text-sm">{t("modelAliases.pull.empty")}</p>
                <div className="flex gap-2 justify-end">
                  <Button variant="neutral" onClick={closePull}>{t("common.cancel")}</Button>
                </div>
              </div>
            ) : (
              <div className="space-y-3">
                <p className="text-sm">{t("modelAliases.pull.found", { count: pulledModels.length })}</p>
                <div className="flex gap-2">
                  <Button
                    variant="neutral"
                    onClick={() =>
                      setPullSelected((prev) =>
                        prev.size === pulledModels.length ? new Set() : new Set(pulledModels),
                      )
                    }
                  >
                    {pullSelected.size === pulledModels.length
                      ? t("modelAliases.pull.deselectAll")
                      : t("modelAliases.pull.selectAll")}
                  </Button>
                </div>
                <div className="max-h-60 overflow-y-auto space-y-1 border border-border rounded p-2">
                  {pulledModels.map((model) => (
                    <label key={model} className="flex items-center gap-2 cursor-pointer text-sm py-0.5">
                      <input
                        type="checkbox"
                        checked={pullSelected.has(model)}
                        onChange={() =>
                          setPullSelected((prev) => {
                            const next = new Set(prev);
                            if (next.has(model)) next.delete(model);
                            else next.add(model);
                            return next;
                          })
                        }
                      />
                      {model}
                    </label>
                  ))}
                </div>
                <div className="flex gap-2 justify-end">
                  <Button variant="neutral" onClick={closePull}>{t("common.cancel")}</Button>
                  <Button onClick={() => void importSelected()} disabled={pullSelected.size === 0}>
                    {t("modelAliases.pull.importSelected")} ({pullSelected.size})
                  </Button>
                </div>
              </div>
            )}
          </div>
        </div>
      )}
    </Card>
  );
}

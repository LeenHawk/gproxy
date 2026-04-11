import { useEffect, useMemo, useState } from "react";

import { useI18n } from "../../app/i18n";
import { Button, Card, Input, Label, Select } from "../../components/ui";
import { apiJson, apiVoid } from "../../lib/api";
import { authHeaders } from "../../lib/auth";
import { parseRequiredI64 } from "../../lib/form";
import type { MemoryModelAliasRow, ModelAliasWrite, ProviderRow } from "../../lib/types/admin";

type PullModelsResponse = { models: string[] };

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
  const [rows, setRows] = useState<MemoryModelAliasRow[]>([]);
  const [selectedAlias, setSelectedAlias] = useState<string | null>(null);
  const [form, setForm] = useState({
    id: "",
    alias: "",
    provider_id: "",
    model_id: "",
    enabled: true,
  });
  const nextId = useMemo(
    () => rows.reduce((max, row) => Math.max(max, row.id), 0) + 1,
    [rows],
  );

  // Pull modal state
  const [pullOpen, setPullOpen] = useState(false);
  const [pullProviderId, setPullProviderId] = useState("");
  const [pullLoading, setPullLoading] = useState(false);
  const [pulledModels, setPulledModels] = useState<string[] | null>(null);
  const [pullSelected, setPullSelected] = useState<Set<string>>(new Set());

  const beginCreate = () => {
    setSelectedAlias(null);
    setForm({
      id: String(nextId),
      alias: "",
      provider_id: providers[0] ? String(providers[0].id) : "",
      model_id: "",
      enabled: true,
    });
  };

  const load = async () => {
    const [providerRows, aliasRows] = await Promise.all([
      apiJson<ProviderRow[]>("/admin/providers/query", {
        method: "POST",
        headers,
        body: JSON.stringify({}),
      }),
      apiJson<MemoryModelAliasRow[]>("/admin/model-aliases/query", {
        method: "POST",
        headers,
        body: JSON.stringify({}),
      }),
    ]);
    setProviders(providerRows);
    setRows(aliasRows);
  };

  useEffect(() => {
    void load().catch((error) => notify("error", error instanceof Error ? error.message : String(error)));
  }, []);

  useEffect(() => {
    if (!selectedAlias && !form.id && providers.length > 0) {
      beginCreate();
    }
  }, [form.id, providers, selectedAlias]);

  const save = async () => {
    try {
      const payload: ModelAliasWrite = {
        id: parseRequiredI64(form.id, "id"),
        alias: form.alias.trim(),
        provider_id: parseRequiredI64(form.provider_id, "provider_id"),
        model_id: form.model_id.trim(),
        enabled: form.enabled,
      };
      await apiJson("/admin/model-aliases/upsert", {
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

  const remove = async (alias: string) => {
    try {
      await apiVoid("/admin/model-aliases/delete", {
        method: "POST",
        headers,
        body: JSON.stringify({ alias }),
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
      const resp = await apiJson<PullModelsResponse>("/admin/model-aliases/pull", {
        method: "POST",
        headers,
        body: JSON.stringify({ provider_id: Number(pullProviderId) }),
      });

      // Filter out models that already exist as aliases
      const existingAliases = new Set(rows.map((row) => row.alias));
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
      const maxId = rows.reduce((max, row) => Math.max(max, row.id), 0);
      const items: ModelAliasWrite[] = [...pullSelected].map((model, index) => ({
        id: maxId + index + 1,
        alias: model,
        provider_id: Number(pullProviderId),
        model_id: model,
        enabled: true,
      }));
      await apiJson("/admin/model-aliases/batch-upsert", {
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
          {rows.map((row) => (
            <div
              key={row.id}
              className={`card-shell cursor-pointer ${row.alias === selectedAlias ? "nav-item-active" : ""}`}
              onClick={() => {
                setSelectedAlias(row.alias);
                const provider = providers.find((item) => item.name === row.provider_name);
                setForm({
                  id: String(row.id),
                  alias: row.alias,
                  provider_id: provider ? String(provider.id) : "",
                  model_id: row.model_id,
                  enabled: true,
                });
              }}
            >
              <div className="font-semibold">{row.alias}</div>
              <div className="text-xs text-muted">{row.provider_name} / {row.model_id}</div>
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
            {selectedAlias ? <Button variant="danger" onClick={() => void remove(selectedAlias)}>{t("common.delete")}</Button> : null}
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

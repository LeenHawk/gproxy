import { useEffect, useMemo, useState } from "react";

import { useI18n } from "../../app/i18n";
import { Button, Card, Input, Label, Select } from "../../components/ui";
import { apiJson, apiVoid } from "../../lib/api";
import { authHeaders } from "../../lib/auth";
import { parseRequiredI64 } from "../../lib/form";
import type { MemoryModelAliasRow, ModelAliasWrite, ProviderRow } from "../../lib/types/admin";

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

  return (
    <Card title={t("modelAliases.title")}>
      <div className="grid gap-4 xl:grid-cols-[360px_minmax(0,1fr)]">
        <div className="space-y-2">
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
    </Card>
  );
}

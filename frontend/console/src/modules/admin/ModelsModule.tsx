import { useEffect, useMemo, useState } from "react";

import { useI18n } from "../../app/i18n";
import { Button, Card, Input, Label, Select, TextArea } from "../../components/ui";
import { apiJson, apiVoid } from "../../lib/api";
import { authHeaders } from "../../lib/auth";
import { parseRequiredI64 } from "../../lib/form";
import type { MemoryModelRow, ModelWrite, ProviderRow } from "../../lib/types/admin";

export function ModelsModule({
  sessionToken,
  notify,
}: {
  sessionToken: string;
  notify: (kind: "success" | "error" | "info", message: string) => void;
}) {
  const { t } = useI18n();
  const headers = useMemo(() => authHeaders(sessionToken), [sessionToken]);
  const [providers, setProviders] = useState<ProviderRow[]>([]);
  const [rows, setRows] = useState<MemoryModelRow[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [form, setForm] = useState({
    id: "",
    provider_id: "",
    model_id: "",
    display_name: "",
    enabled: true,
    price_each_call: "",
    price_tiers_json: "[]",
  });

  const selected = rows.find((row) => row.id === selectedId) ?? null;
  const nextId = useMemo(
    () => rows.reduce((max, row) => Math.max(max, row.id), 0) + 1,
    [rows],
  );

  const beginCreate = () => {
    setSelectedId(null);
    setForm({
      id: String(nextId),
      provider_id: providers[0] ? String(providers[0].id) : "",
      model_id: "",
      display_name: "",
      enabled: true,
      price_each_call: "",
      price_tiers_json: "[]",
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
    setRows(modelRows);
  };

  useEffect(() => {
    void load().catch((error) => notify("error", error instanceof Error ? error.message : String(error)));
  }, []);

  useEffect(() => {
    if (!selectedId && !form.id && providers.length > 0) {
      beginCreate();
    }
  }, [form.id, providers, selectedId]);

  const save = async () => {
    try {
      const payload: ModelWrite = {
        id: parseRequiredI64(form.id, "id"),
        provider_id: parseRequiredI64(form.provider_id, "provider_id"),
        model_id: form.model_id.trim(),
        display_name: form.display_name.trim() || null,
        enabled: form.enabled,
        price_each_call: form.price_each_call.trim() ? Number(form.price_each_call) : null,
        price_tiers_json: form.price_tiers_json,
      };
      await apiJson("/admin/models/upsert", {
        method: "POST",
        headers,
        body: JSON.stringify(payload),
      });
      notify("success", t("models.saved"));
      await load();
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  const remove = async (id: number) => {
    try {
      await apiVoid("/admin/models/delete", {
        method: "POST",
        headers,
        body: JSON.stringify({ id }),
      });
      notify("success", t("models.deleted"));
      await load();
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <Card title={t("models.title")}>
      <div className="grid gap-4 lg:grid-cols-[320px_minmax(0,1fr)]">
        <div className="space-y-2">
          {rows.map((row) => (
            <div
              key={row.id}
              className={`card-shell cursor-pointer ${row.id === selectedId ? "nav-item-active" : ""}`}
              onClick={() => {
                setSelectedId(row.id);
                setForm({
                  id: String(row.id),
                  provider_id: String(row.provider_id),
                  model_id: row.model_id,
                  display_name: row.display_name ?? "",
                  enabled: row.enabled,
                  price_each_call: row.price_each_call?.toString() ?? "",
                  price_tiers_json: JSON.stringify(row.price_tiers, null, 2),
                });
              }}
            >
              <div className="font-semibold">{row.model_id}</div>
              <div className="text-xs text-muted">#{row.id} · provider #{row.provider_id}</div>
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
              options={providers.map((provider) => ({ value: String(provider.id), label: `${provider.name} (#${provider.id})` }))}
            />
          </div>
          <div>
            <Label>{t("common.modelId")}</Label>
            <Input value={form.model_id} onChange={(value) => setForm((current) => ({ ...current, model_id: value }))} />
          </div>
          <div>
            <Label>{t("common.displayName")}</Label>
            <Input value={form.display_name} onChange={(value) => setForm((current) => ({ ...current, display_name: value }))} />
          </div>
          <label className="flex items-center gap-2 text-sm text-muted">
            <input type="checkbox" checked={form.enabled} onChange={(event) => setForm((current) => ({ ...current, enabled: event.target.checked }))} />
            {t("common.enabled")}
          </label>
          <div>
            <Label>{t("common.priceEachCall")}</Label>
            <Input value={form.price_each_call} onChange={(value) => setForm((current) => ({ ...current, price_each_call: value }))} />
          </div>
          <div>
            <Label>{t("common.priceTiersJson")}</Label>
            <TextArea value={form.price_tiers_json} onChange={(value) => setForm((current) => ({ ...current, price_tiers_json: value }))} rows={8} />
          </div>
          <div className="flex gap-2">
            <Button onClick={() => void save()}>{t("common.save")}</Button>
            {selected ? <Button variant="danger" onClick={() => void remove(selected.id)}>{t("common.delete")}</Button> : null}
          </div>
        </div>
      </div>
    </Card>
  );
}

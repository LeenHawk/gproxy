import React, { useCallback, useEffect, useState } from "react";
import Panel from "../components/Panel";
import JsonBlock from "../components/JsonBlock";
import { apiErrorMessage, apiRequest } from "../lib/api";
import { formatTimestamp } from "../lib/format";
import type { Provider } from "../lib/types";

type ProviderFormState = {
  name: string;
  enabled: boolean;
  configText: string;
};

const emptyForm: ProviderFormState = {
  name: "",
  enabled: true,
  configText: "{}"
};

function parseConfig(text: string): unknown {
  if (!text.trim()) {
    return {};
  }
  return JSON.parse(text);
}

export default function ProvidersSection({
  adminKey,
  notify
}: {
  adminKey: string;
  notify: (toast: { type: "success" | "error" | "info" | "warning"; message: string }) => void;
}) {
  const [providers, setProviders] = useState<Provider[]>([]);
  const [loading, setLoading] = useState(false);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [form, setForm] = useState<ProviderFormState>(emptyForm);
  const [expanded, setExpanded] = useState<Record<number, boolean>>({});

  const loadProviders = useCallback(async () => {
    setLoading(true);
    try {
      const data = await apiRequest<Provider[]>("/admin/providers", { adminKey });
      setProviders(Array.isArray(data) ? data : []);
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    } finally {
      setLoading(false);
    }
  }, [adminKey, notify]);

  useEffect(() => {
    loadProviders();
  }, [loadProviders]);

  const resetForm = () => {
    setForm(emptyForm);
    setEditingId(null);
  };

  const handleEdit = (provider: Provider) => {
    setEditingId(provider.id);
    setForm({
      name: provider.name,
      enabled: provider.enabled,
      configText: JSON.stringify(provider.config_json ?? {}, null, 2)
    });
  };

  const handleSubmit = async () => {
    try {
      const payload = {
        id: editingId ?? undefined,
        name: form.name.trim(),
        enabled: form.enabled,
        config_json: parseConfig(form.configText)
      };
      if (!payload.name) {
        throw new Error("Provider name is required.");
      }
      if (editingId) {
        await apiRequest(`/admin/providers/${editingId}`, {
          method: "PUT",
          body: payload,
          adminKey
        });
        notify({ type: "success", message: "Provider updated." });
      } else {
        await apiRequest("/admin/providers", {
          method: "POST",
          body: payload,
          adminKey
        });
        notify({ type: "success", message: "Provider created." });
      }
      resetForm();
      await loadProviders();
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    }
  };

  const handleDelete = async (provider: Provider) => {
    if (!confirm(`Delete provider ${provider.name}?`)) {
      return;
    }
    try {
      await apiRequest(`/admin/providers/${provider.id}`, { method: "DELETE", adminKey });
      notify({ type: "success", message: "Provider deleted." });
      await loadProviders();
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    }
  };

  return (
    <div className="space-y-6">
      <Panel
        title={editingId ? "Edit provider" : "Create provider"}
        subtitle="Manage provider definitions and config payloads."
        action={
          editingId ? (
            <button className="btn btn-ghost" type="button" onClick={resetForm}>
              Cancel edit
            </button>
          ) : null
        }
      >
        <div className="grid gap-4 md:grid-cols-3">
          <div>
            <label className="label">Provider name</label>
            <input
              className="input"
              value={form.name}
              onChange={(event) => setForm((prev) => ({ ...prev, name: event.target.value }))}
              placeholder="openai"
            />
          </div>
          <div>
            <label className="label">Enabled</label>
            <select
              className="select"
              value={form.enabled ? "true" : "false"}
              onChange={(event) =>
                setForm((prev) => ({ ...prev, enabled: event.target.value === "true" }))
              }
            >
              <option value="true">Enabled</option>
              <option value="false">Disabled</option>
            </select>
          </div>
          <div className="flex items-end">
            <button className="btn btn-primary w-full" type="button" onClick={handleSubmit}>
              {editingId ? "Save provider" : "Create provider"}
            </button>
          </div>
        </div>
        <div>
          <label className="label">Config JSON</label>
          <textarea
            className="textarea"
            rows={6}
            value={form.configText}
            onChange={(event) => setForm((prev) => ({ ...prev, configText: event.target.value }))}
            placeholder='{"base_url": "https://api"}'
          />
        </div>
      </Panel>

      <Panel
        title="Providers"
        subtitle="Stored providers and current config payloads."
        action={
          <button className="btn btn-ghost" type="button" onClick={loadProviders}>
            Refresh
          </button>
        }
      >
        {loading ? (
          <div className="text-sm text-slate-500">Loading providers...</div>
        ) : providers.length === 0 ? (
          <div className="text-sm text-slate-500">No providers registered.</div>
        ) : (
          <div className="space-y-3">
            {providers.map((provider) => (
              <div
                key={provider.id}
                className="rounded-2xl border border-slate-200 bg-white/90 p-4"
              >
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div>
                    <div className="text-sm font-semibold text-slate-800">{provider.name}</div>
                    <div className="mt-1 text-xs text-slate-400">ID {provider.id}</div>
                  </div>
                  <div className="flex flex-wrap items-center gap-2">
                    <span
                      className={`badge ${
                        provider.enabled
                          ? "border-emerald-200 text-emerald-600"
                          : "border-slate-200 text-slate-500"
                      }`}
                    >
                      {provider.enabled ? "Enabled" : "Disabled"}
                    </span>
                    <button className="btn btn-ghost" type="button" onClick={() => handleEdit(provider)}>
                      Edit
                    </button>
                    <button
                      className="btn btn-ghost"
                      type="button"
                      onClick={() =>
                        setExpanded((prev) => ({
                          ...prev,
                          [provider.id]: !prev[provider.id]
                        }))
                      }
                    >
                      {expanded[provider.id] ? "Hide" : "View"}
                    </button>
                    <button
                      className="btn btn-danger"
                      type="button"
                      onClick={() => handleDelete(provider)}
                    >
                      Delete
                    </button>
                  </div>
                </div>
                <div className="mt-3 grid gap-2 text-xs text-slate-500 md:grid-cols-2">
                  <div>Updated</div>
                  <div className="text-right text-slate-700">
                    {formatTimestamp(provider.updated_at)}
                  </div>
                </div>
                {expanded[provider.id] ? (
                  <div className="mt-3">
                    <JsonBlock value={provider.config_json} />
                  </div>
                ) : null}
              </div>
            ))}
          </div>
        )}
      </Panel>
    </div>
  );
}

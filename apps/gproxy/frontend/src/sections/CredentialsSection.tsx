import React, { useCallback, useDeferredValue, useEffect, useMemo, useState } from "react";
import Panel from "../components/Panel";
import JsonBlock from "../components/JsonBlock";
import { apiErrorMessage, apiRequest } from "../lib/api";
import { formatTimestamp, maskValue } from "../lib/format";
import type { Credential, Provider } from "../lib/types";

type CredentialFormState = {
  providerId: string;
  name: string;
  weight: string;
  enabled: boolean;
  secretText: string;
  metaText: string;
};

const emptyForm: CredentialFormState = {
  providerId: "",
  name: "",
  weight: "1",
  enabled: true,
  secretText: "{}",
  metaText: "{}"
};

const readFileAsText = (file: File) =>
  new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result ?? ""));
    reader.onerror = () => reject(reader.error);
    reader.readAsText(file);
  });

function parseJson(text: string, fallback: unknown) {
  if (!text.trim()) {
    return fallback;
  }
  return JSON.parse(text);
}

export default function CredentialsSection({
  adminKey,
  notify
}: {
  adminKey: string;
  notify: (toast: { type: "success" | "error" | "info" | "warning"; message: string }) => void;
}) {
  const [providers, setProviders] = useState<Provider[]>([]);
  const [credentials, setCredentials] = useState<Credential[]>([]);
  const [loading, setLoading] = useState(false);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [form, setForm] = useState<CredentialFormState>(emptyForm);
  const [expanded, setExpanded] = useState<Record<number, boolean>>({});
  const [search, setSearch] = useState("");
  const [filterProvider, setFilterProvider] = useState("all");
  const [batchText, setBatchText] = useState("");
  const [batchFiles, setBatchFiles] = useState<File[]>([]);
  const deferredSearch = useDeferredValue(search);

  const loadProviders = useCallback(async () => {
    try {
      const data = await apiRequest<Provider[]>("/admin/providers", { adminKey });
      const items = Array.isArray(data) ? data : [];
      setProviders(items);
      if (!form.providerId && items.length > 0) {
        setForm((prev) => ({ ...prev, providerId: String(items[0].id) }));
      }
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    }
  }, [adminKey, form.providerId, notify]);

  const loadCredentials = useCallback(async () => {
    setLoading(true);
    try {
      const data = await apiRequest<Credential[]>("/admin/credentials", { adminKey });
      setCredentials(Array.isArray(data) ? data : []);
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    } finally {
      setLoading(false);
    }
  }, [adminKey, notify]);

  useEffect(() => {
    loadProviders();
    loadCredentials();
  }, [loadCredentials, loadProviders]);

  const providerMap = useMemo(() => {
    const map = new Map<number, Provider>();
    providers.forEach((provider) => map.set(provider.id, provider));
    return map;
  }, [providers]);

  const filteredCredentials = useMemo(() => {
    const term = deferredSearch.trim().toLowerCase();
    return credentials.filter((cred) => {
      if (filterProvider !== "all" && String(cred.provider_id) !== filterProvider) {
        return false;
      }
      if (!term) {
        return true;
      }
      const providerName = providerMap.get(cred.provider_id)?.name ?? "";
      return (
        String(cred.id).includes(term) ||
        providerName.toLowerCase().includes(term) ||
        (cred.name ?? "").toLowerCase().includes(term)
      );
    });
  }, [credentials, deferredSearch, filterProvider, providerMap]);

  const resetForm = () => {
    setEditingId(null);
    setForm((prev) => ({ ...emptyForm, providerId: prev.providerId || emptyForm.providerId }));
  };

  const handleEdit = (credential: Credential) => {
    setEditingId(credential.id);
    setForm({
      providerId: String(credential.provider_id),
      name: credential.name ?? "",
      weight: String(credential.weight ?? 1),
      enabled: credential.enabled,
      secretText: JSON.stringify(credential.secret ?? {}, null, 2),
      metaText: JSON.stringify(credential.meta_json ?? {}, null, 2)
    });
  };

  const handleSubmit = async () => {
    try {
      if (!form.providerId) {
        throw new Error("Provider is required.");
      }
      const payload = {
        id: editingId ?? undefined,
        provider_id: Number(form.providerId),
        name: form.name.trim() ? form.name.trim() : null,
        secret: parseJson(form.secretText, {}),
        meta_json: parseJson(form.metaText, {}),
        weight: Number(form.weight || 1),
        enabled: form.enabled
      };
      if (Number.isNaN(payload.weight)) {
        throw new Error("Weight must be a number.");
      }
      if (editingId) {
        await apiRequest(`/admin/credentials/${editingId}`, {
          method: "PUT",
          body: payload,
          adminKey
        });
        notify({ type: "success", message: "Credential updated." });
      } else {
        await apiRequest("/admin/credentials", {
          method: "POST",
          body: payload,
          adminKey
        });
        notify({ type: "success", message: "Credential created." });
      }
      resetForm();
      await loadCredentials();
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    }
  };

  const handleDelete = async (credential: Credential) => {
    if (!confirm(`Delete credential ${credential.id}?`)) {
      return;
    }
    try {
      await apiRequest(`/admin/credentials/${credential.id}`, {
        method: "DELETE",
        adminKey
      });
      notify({ type: "success", message: "Credential deleted." });
      await loadCredentials();
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    }
  };

  const handleBatchUpload = async () => {
    try {
      const payloads: unknown[] = [];
      if (batchText.trim()) {
        const parsed = JSON.parse(batchText);
        if (Array.isArray(parsed)) {
          payloads.push(...parsed);
        } else {
          payloads.push(parsed);
        }
      }
      for (const file of batchFiles) {
        const text = await readFileAsText(file);
        const parsed = JSON.parse(text);
        if (Array.isArray(parsed)) {
          payloads.push(...parsed);
        } else {
          payloads.push(parsed);
        }
      }
      if (payloads.length === 0) {
        throw new Error("No payloads found.");
      }
      for (const payload of payloads) {
        await apiRequest("/admin/credentials", {
          method: "POST",
          body: payload,
          adminKey
        });
      }
      setBatchText("");
      setBatchFiles([]);
      notify({ type: "success", message: `Uploaded ${payloads.length} credential(s).` });
      await loadCredentials();
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    }
  };

  return (
    <div className="space-y-6">
      <Panel
        title={editingId ? "Edit credential" : "Add credential"}
        subtitle="Create or update provider credentials and secrets."
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
            <label className="label">Provider</label>
            <select
              className="select"
              value={form.providerId}
              onChange={(event) =>
                setForm((prev) => ({ ...prev, providerId: event.target.value }))
              }
            >
              {providers.map((provider) => (
                <option key={provider.id} value={provider.id}>
                  {provider.name} (#{provider.id})
                </option>
              ))}
            </select>
          </div>
          <div>
            <label className="label">Label</label>
            <input
              className="input"
              value={form.name}
              onChange={(event) => setForm((prev) => ({ ...prev, name: event.target.value }))}
              placeholder="Optional label"
            />
          </div>
          <div>
            <label className="label">Weight</label>
            <input
              className="input"
              type="number"
              value={form.weight}
              onChange={(event) => setForm((prev) => ({ ...prev, weight: event.target.value }))}
              min={0}
            />
          </div>
        </div>
        <div className="grid gap-4 md:grid-cols-2">
          <div>
            <label className="label">Secret JSON</label>
            <textarea
              className="textarea"
              rows={6}
              value={form.secretText}
              onChange={(event) => setForm((prev) => ({ ...prev, secretText: event.target.value }))}
            />
          </div>
          <div>
            <label className="label">Meta JSON</label>
            <textarea
              className="textarea"
              rows={6}
              value={form.metaText}
              onChange={(event) => setForm((prev) => ({ ...prev, metaText: event.target.value }))}
            />
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-3">
          <label className="flex items-center gap-2 text-sm text-slate-600">
            <input
              type="checkbox"
              checked={form.enabled}
              onChange={(event) => setForm((prev) => ({ ...prev, enabled: event.target.checked }))}
            />
            Enabled
          </label>
          <button className="btn btn-primary" type="button" onClick={handleSubmit}>
            {editingId ? "Save credential" : "Create credential"}
          </button>
        </div>
      </Panel>

      <Panel
        title="Batch upload"
        subtitle="Paste an array of credential payloads or upload JSON files."
      >
        <div className="grid gap-4 md:grid-cols-2">
          <div>
            <label className="label">JSON payloads</label>
            <textarea
              className="textarea"
              rows={6}
              value={batchText}
              onChange={(event) => setBatchText(event.target.value)}
              placeholder='[{"provider_id":1,"secret":{}}]'
            />
          </div>
          <div>
            <label className="label">Upload files</label>
            <div className="mt-2 flex flex-wrap items-center gap-3">
              <label className="btn btn-primary cursor-pointer">
                Select files
                <input
                  type="file"
                  multiple
                  className="hidden"
                  onChange={(event) =>
                    setBatchFiles(Array.from(event.target.files ?? []))
                  }
                />
              </label>
              <div className="text-xs text-slate-500">
                {batchFiles.length ? `${batchFiles.length} file(s) selected.` : "No files selected."}
              </div>
            </div>
          </div>
        </div>
        <button className="btn btn-accent" type="button" onClick={handleBatchUpload}>
          Upload batch
        </button>
      </Panel>

      <Panel
        title="Credentials"
        subtitle="Manage existing credentials across providers."
        action={
          <button className="btn btn-ghost" type="button" onClick={loadCredentials}>
            Refresh
          </button>
        }
      >
        <div className="grid gap-3 md:grid-cols-3">
          <div>
            <label className="label">Search</label>
            <input
              className="input"
              value={search}
              onChange={(event) => setSearch(event.target.value)}
              placeholder="ID, provider, label..."
            />
          </div>
          <div>
            <label className="label">Provider filter</label>
            <select
              className="select"
              value={filterProvider}
              onChange={(event) => setFilterProvider(event.target.value)}
            >
              <option value="all">All providers</option>
              {providers.map((provider) => (
                <option key={provider.id} value={provider.id}>
                  {provider.name}
                </option>
              ))}
            </select>
          </div>
        </div>

        {loading ? (
          <div className="text-sm text-slate-500">Loading credentials...</div>
        ) : filteredCredentials.length === 0 ? (
          <div className="text-sm text-slate-500">No credentials found.</div>
        ) : (
          <div className="space-y-3">
            {filteredCredentials.map((credential) => {
              const providerName = providerMap.get(credential.provider_id)?.name ?? "Unknown";
              return (
                <div
                  key={credential.id}
                  className="rounded-2xl border border-slate-200 bg-white/90 p-4"
                >
                  <div className="flex flex-wrap items-start justify-between gap-3">
                    <div>
                      <div className="text-sm font-semibold text-slate-800">
                        {providerName} · #{credential.id}
                      </div>
                      <div className="mt-1 text-xs text-slate-400">
                        {credential.name || maskValue(String(credential.id))}
                      </div>
                    </div>
                    <div className="flex flex-wrap items-center gap-2">
                      <span
                        className={`badge ${
                          credential.enabled
                            ? "border-emerald-200 text-emerald-600"
                            : "border-slate-200 text-slate-500"
                        }`}
                      >
                        {credential.enabled ? "Enabled" : "Disabled"}
                      </span>
                      <button className="btn btn-ghost" type="button" onClick={() => handleEdit(credential)}>
                        Edit
                      </button>
                      <button
                        className="btn btn-ghost"
                        type="button"
                        onClick={() =>
                          setExpanded((prev) => ({
                            ...prev,
                            [credential.id]: !prev[credential.id]
                          }))
                        }
                      >
                        {expanded[credential.id] ? "Hide" : "View"}
                      </button>
                      <button
                        className="btn btn-danger"
                        type="button"
                        onClick={() => handleDelete(credential)}
                      >
                        Delete
                      </button>
                    </div>
                  </div>
                  <div className="mt-3 grid gap-2 text-xs text-slate-500 md:grid-cols-2">
                    <div>Weight</div>
                    <div className="text-right text-slate-700">{credential.weight}</div>
                    <div>Created</div>
                    <div className="text-right text-slate-700">
                      {formatTimestamp(credential.created_at)}
                    </div>
                    <div>Updated</div>
                    <div className="text-right text-slate-700">
                      {formatTimestamp(credential.updated_at)}
                    </div>
                  </div>
                  {expanded[credential.id] ? (
                    <div className="mt-3 grid gap-3 md:grid-cols-2">
                      <div>
                        <div className="label">Secret</div>
                        <JsonBlock value={credential.secret} />
                      </div>
                      <div>
                        <div className="label">Meta</div>
                        <JsonBlock value={credential.meta_json} />
                      </div>
                    </div>
                  ) : null}
                </div>
              );
            })}
          </div>
        )}
      </Panel>
    </div>
  );
}

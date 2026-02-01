import React, { useCallback, useEffect, useState } from "react";
import Panel from "../components/Panel";
import { apiErrorMessage, apiRequest } from "../lib/api";
import { formatTimestamp, fromEpochSeconds, toEpochSeconds } from "../lib/format";
import type { Credential, DisallowRecord } from "../lib/types";

type FormState = {
  credentialId: string;
  scopeKind: string;
  scopeValue: string;
  level: string;
  untilAt: string;
  reason: string;
};

const emptyForm: FormState = {
  credentialId: "",
  scopeKind: "all_models",
  scopeValue: "",
  level: "cooldown",
  untilAt: "",
  reason: ""
};

export default function DisallowSection({
  adminKey,
  notify
}: {
  adminKey: string;
  notify: (toast: { type: "success" | "error" | "info" | "warning"; message: string }) => void;
}) {
  const [records, setRecords] = useState<DisallowRecord[]>([]);
  const [credentials, setCredentials] = useState<Credential[]>([]);
  const [form, setForm] = useState<FormState>(emptyForm);
  const [loading, setLoading] = useState(false);

  const loadDisallow = useCallback(async () => {
    setLoading(true);
    try {
      const data = await apiRequest<DisallowRecord[]>("/admin/disallow", { adminKey });
      setRecords(Array.isArray(data) ? data : []);
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    } finally {
      setLoading(false);
    }
  }, [adminKey, notify]);

  const loadCredentials = useCallback(async () => {
    try {
      const data = await apiRequest<Credential[]>("/admin/credentials", { adminKey });
      const items = Array.isArray(data) ? data : [];
      setCredentials(items);
      if (!form.credentialId && items.length > 0) {
        setForm((prev) => ({ ...prev, credentialId: String(items[0].id) }));
      }
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    }
  }, [adminKey, form.credentialId, notify]);

  useEffect(() => {
    loadDisallow();
    loadCredentials();
  }, [loadDisallow, loadCredentials]);

  const handleSubmit = async () => {
    try {
      if (!form.credentialId) {
        throw new Error("Credential is required.");
      }
      if (form.scopeKind === "model" && !form.scopeValue.trim()) {
        throw new Error("Model scope requires a value.");
      }
      const payload = {
        credential_id: Number(form.credentialId),
        scope_kind: form.scopeKind,
        scope_value: form.scopeKind === "model" ? form.scopeValue.trim() : null,
        level: form.level,
        until_at: toEpochSeconds(form.untilAt),
        reason: form.reason.trim() ? form.reason.trim() : null
      };
      await apiRequest("/admin/disallow", {
        method: "POST",
        body: payload,
        adminKey
      });
      notify({ type: "success", message: "Disallow rule added." });
      setForm(emptyForm);
      await loadDisallow();
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    }
  };

  const handleDelete = async (record: DisallowRecord) => {
    if (!confirm(`Delete disallow record ${record.id}?`)) {
      return;
    }
    try {
      await apiRequest(`/admin/disallow/${record.id}`, { method: "DELETE", adminKey });
      notify({ type: "success", message: "Disallow record deleted." });
      await loadDisallow();
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    }
  };

  const credentialMap = new Map(credentials.map((cred) => [cred.id, cred]));

  return (
    <div className="space-y-6">
      <Panel title="Add disallow" subtitle="Limit a credential for a model or all models.">
        <div className="grid gap-4 md:grid-cols-3">
          <div>
            <label className="label">Credential</label>
            <select
              className="select"
              value={form.credentialId}
              onChange={(event) => setForm((prev) => ({ ...prev, credentialId: event.target.value }))}
            >
              {credentials.map((cred) => (
                <option key={cred.id} value={cred.id}>
                  #{cred.id} {cred.name || ""}
                </option>
              ))}
            </select>
          </div>
          <div>
            <label className="label">Scope</label>
            <select
              className="select"
              value={form.scopeKind}
              onChange={(event) => setForm((prev) => ({ ...prev, scopeKind: event.target.value }))}
            >
              <option value="all_models">All models</option>
              <option value="model">Specific model</option>
            </select>
          </div>
          <div>
            <label className="label">Level</label>
            <select
              className="select"
              value={form.level}
              onChange={(event) => setForm((prev) => ({ ...prev, level: event.target.value }))}
            >
              <option value="cooldown">Cooldown</option>
              <option value="transient">Transient</option>
              <option value="dead">Dead</option>
            </select>
          </div>
        </div>
        {form.scopeKind === "model" ? (
          <div>
            <label className="label">Model</label>
            <input
              className="input"
              value={form.scopeValue}
              onChange={(event) => setForm((prev) => ({ ...prev, scopeValue: event.target.value }))}
              placeholder="gpt-4.1"
            />
          </div>
        ) : null}
        <div className="grid gap-4 md:grid-cols-2">
          <div>
            <label className="label">Until (optional)</label>
            <input
              className="input"
              type="datetime-local"
              value={form.untilAt}
              onChange={(event) => setForm((prev) => ({ ...prev, untilAt: event.target.value }))}
            />
          </div>
          <div>
            <label className="label">Reason</label>
            <input
              className="input"
              value={form.reason}
              onChange={(event) => setForm((prev) => ({ ...prev, reason: event.target.value }))}
              placeholder="Optional reason"
            />
          </div>
        </div>
        <button className="btn btn-primary" type="button" onClick={handleSubmit}>
          Add disallow
        </button>
      </Panel>

      <Panel
        title="Disallow records"
        subtitle="Active restrictions for credentials."
        action={
          <button className="btn btn-ghost" type="button" onClick={loadDisallow}>
            Refresh
          </button>
        }
      >
        {loading ? (
          <div className="text-sm text-slate-500">Loading disallow records...</div>
        ) : records.length === 0 ? (
          <div className="text-sm text-slate-500">No disallow rules.</div>
        ) : (
          <div className="space-y-3">
            {records.map((record) => {
              const credential = credentialMap.get(record.credential_id);
              return (
                <div key={record.id} className="rounded-2xl border border-slate-200 bg-white/90 p-4">
                  <div className="flex flex-wrap items-start justify-between gap-3">
                    <div>
                      <div className="text-sm font-semibold text-slate-800">
                        Credential #{record.credential_id}
                      </div>
                      <div className="mt-1 text-xs text-slate-400">
                        {credential?.name || "No label"}
                      </div>
                    </div>
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="badge border-amber-200 text-amber-600">
                        {record.level}
                      </span>
                      <button
                        className="btn btn-danger"
                        type="button"
                        onClick={() => handleDelete(record)}
                      >
                        Delete
                      </button>
                    </div>
                  </div>
                  <div className="mt-3 grid gap-2 text-xs text-slate-500 md:grid-cols-2">
                    <div>Scope</div>
                    <div className="text-right text-slate-700">
                      {record.scope_kind === "model"
                        ? `Model: ${record.scope_value ?? "-"}`
                        : "All models"}
                    </div>
                    <div>Until</div>
                    <div className="text-right text-slate-700">
                      {record.until_at ? fromEpochSeconds(record.until_at) : "-"}
                    </div>
                    <div>Updated</div>
                    <div className="text-right text-slate-700">
                      {formatTimestamp(record.updated_at)}
                    </div>
                    <div>Reason</div>
                    <div className="text-right text-slate-700">
                      {record.reason || "-"}
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </Panel>
    </div>
  );
}

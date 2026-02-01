import React, { useCallback, useEffect, useState } from "react";
import Panel from "../components/Panel";
import { apiErrorMessage, apiRequest } from "../lib/api";
import { formatTimestamp } from "../lib/format";
import type { GlobalConfig } from "../lib/types";

type ConfigResponse = {
  config_json: GlobalConfig;
  updated_at: number;
};

export default function ConfigSection({
  adminKey,
  notify,
  onAdminKeyUpdate
}: {
  adminKey: string;
  notify: (toast: { type: "success" | "error" | "info" | "warning"; message: string }) => void;
  onAdminKeyUpdate: (nextKey: string) => void;
}) {
  const [config, setConfig] = useState<GlobalConfig | null>(null);
  const [draft, setDraft] = useState<GlobalConfig | null>(null);
  const [updatedAt, setUpdatedAt] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);

  const loadConfig = useCallback(async () => {
    setLoading(true);
    try {
      const data = await apiRequest<ConfigResponse>("/admin/config", { adminKey });
      setConfig(data.config_json);
      setDraft(data.config_json);
      setUpdatedAt(data.updated_at);
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    } finally {
      setLoading(false);
    }
  }, [adminKey, notify]);

  useEffect(() => {
    loadConfig();
  }, [loadConfig]);

  const handleSave = async () => {
    if (!draft) {
      return;
    }
    try {
      const payload: GlobalConfig = {
        ...draft,
        proxy: draft.proxy?.trim() ? draft.proxy : null,
        data_dir: draft.data_dir?.trim() ? draft.data_dir : ""
      };
      await apiRequest("/admin/config", { method: "PUT", body: payload, adminKey });
      notify({ type: "success", message: "Config updated." });
      setConfig(payload);
      if (payload.admin_key && payload.admin_key !== adminKey) {
        onAdminKeyUpdate(payload.admin_key);
      }
      await loadConfig();
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    }
  };

  const handleReload = async () => {
    try {
      await apiRequest("/admin/reload", { method: "POST", adminKey });
      notify({ type: "success", message: "Snapshot reloaded." });
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    }
  };

  if (loading && !draft) {
    return <div className="text-sm text-slate-500">Loading config...</div>;
  }

  if (!draft) {
    return <div className="text-sm text-slate-500">Config not available.</div>;
  }

  return (
    <div className="space-y-6">
      <Panel
        title="Global config"
        subtitle="Editing the admin key will require re-authentication."
        action={
          <button className="btn btn-ghost" type="button" onClick={loadConfig}>
            Refresh
          </button>
        }
      >
        <div className="grid gap-4 md:grid-cols-2">
          <div>
            <label className="label">Host</label>
            <input
              className="input"
              value={draft.host}
              onChange={(event) => setDraft({ ...draft, host: event.target.value })}
            />
          </div>
          <div>
            <label className="label">Port</label>
            <input
              className="input"
              type="number"
              value={draft.port}
              onChange={(event) => setDraft({ ...draft, port: Number(event.target.value) })}
            />
          </div>
          <div>
            <label className="label">Admin key</label>
            <input
              className="input"
              value={draft.admin_key}
              onChange={(event) => setDraft({ ...draft, admin_key: event.target.value })}
            />
          </div>
          <div>
            <label className="label">Proxy</label>
            <input
              className="input"
              value={draft.proxy ?? ""}
              onChange={(event) => setDraft({ ...draft, proxy: event.target.value })}
              placeholder="Optional"
            />
          </div>
          <div className="md:col-span-2">
            <label className="label">DSN</label>
            <input
              className="input"
              value={draft.dsn}
              onChange={(event) => setDraft({ ...draft, dsn: event.target.value })}
            />
          </div>
          <div className="md:col-span-2">
            <label className="label">Data directory</label>
            <input
              className="input"
              value={draft.data_dir ?? ""}
              onChange={(event) => setDraft({ ...draft, data_dir: event.target.value })}
              placeholder="Leave blank to keep current"
            />
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-3">
          <button className="btn btn-primary" type="button" onClick={handleSave}>
            Save config
          </button>
          <button className="btn btn-ghost" type="button" onClick={handleReload}>
            Reload snapshot
          </button>
          {updatedAt ? (
            <span className="text-xs text-slate-500">
              Updated {formatTimestamp(updatedAt)}
            </span>
          ) : null}
        </div>
      </Panel>
    </div>
  );
}

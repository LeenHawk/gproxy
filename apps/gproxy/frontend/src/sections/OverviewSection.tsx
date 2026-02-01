import React, { useCallback, useEffect, useMemo, useState } from "react";
import Panel from "../components/Panel";
import StatCard from "../components/StatCard";
import { apiErrorMessage, apiRequest } from "../lib/api";
import { formatTimestamp } from "../lib/format";
import type { GlobalConfig, ProviderStats } from "../lib/types";

export default function OverviewSection({
  adminKey,
  notify
}: {
  adminKey: string;
  notify: (toast: { type: "success" | "error" | "info" | "warning"; message: string }) => void;
}) {
  const [health, setHealth] = useState<"ok" | "error" | "loading">("loading");
  const [stats, setStats] = useState<ProviderStats[]>([]);
  const [config, setConfig] = useState<GlobalConfig | null>(null);
  const [configUpdatedAt, setConfigUpdatedAt] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);

  const loadOverview = useCallback(async () => {
    setLoading(true);
    try {
      const [healthResp, statsResp, configResp] = await Promise.all([
        apiRequest<{ status: string }>("/admin/health", { adminKey }),
        apiRequest<{ providers: ProviderStats[] }>("/admin/stats", { adminKey }),
        apiRequest<{ config_json: GlobalConfig; updated_at: number }>("/admin/config", { adminKey })
      ]);
      setHealth(healthResp.status === "ok" ? "ok" : "error");
      setStats(statsResp.providers ?? []);
      setConfig(configResp.config_json ?? null);
      setConfigUpdatedAt(configResp.updated_at ?? null);
    } catch (error) {
      setHealth("error");
      notify({ type: "error", message: apiErrorMessage(error) });
    } finally {
      setLoading(false);
    }
  }, [adminKey, notify]);

  useEffect(() => {
    loadOverview();
  }, [loadOverview]);

  const totals = useMemo(() => {
    const providers = stats.length;
    const credentialsTotal = stats.reduce((sum, item) => sum + item.credentials_total, 0);
    const credentialsEnabled = stats.reduce((sum, item) => sum + item.credentials_enabled, 0);
    const disallow = stats.reduce((sum, item) => sum + item.disallow, 0);
    return { providers, credentialsTotal, credentialsEnabled, disallow };
  }, [stats]);

  return (
    <div className="space-y-6">
      <div className="grid gap-4 md:grid-cols-4">
        <StatCard
          label="System health"
          value={health === "loading" ? "Checking" : health === "ok" ? "Healthy" : "Error"}
          hint={health === "ok" ? "Admin key verified" : "Unable to reach admin API"}
        />
        <StatCard label="Providers" value={totals.providers} hint="Available pools" />
        <StatCard label="Credentials" value={totals.credentialsTotal} hint="All stored entries" />
        <StatCard label="Disallow marks" value={totals.disallow} hint="Active restrictions" />
      </div>

      <Panel
        title="Cluster snapshot"
        subtitle="Quick glance at pool health and credential distribution."
        action={
          <button className="btn btn-ghost" type="button" onClick={loadOverview}>
            Refresh snapshot
          </button>
        }
      >
        {loading ? (
          <div className="text-sm text-slate-500">Loading overview...</div>
        ) : (
          <div className="grid gap-3 md:grid-cols-2">
            {stats.map((item) => (
              <div key={item.name} className="rounded-xl border border-slate-200 bg-white/90 p-4">
                <div className="flex items-center justify-between">
                  <div>
                    <div className="text-sm font-semibold text-slate-800">{item.name}</div>
                    <div className="text-xs text-slate-400">Provider pool</div>
                  </div>
                  <span className="badge border-slate-200 text-slate-500">
                    {item.credentials_enabled}/{item.credentials_total} enabled
                  </span>
                </div>
                <div className="mt-3 grid grid-cols-2 gap-2 text-xs text-slate-500">
                  <div>Total credentials</div>
                  <div className="text-right text-slate-700">{item.credentials_total}</div>
                  <div>Enabled credentials</div>
                  <div className="text-right text-slate-700">{item.credentials_enabled}</div>
                  <div>Disallow entries</div>
                  <div className="text-right text-slate-700">{item.disallow}</div>
                </div>
              </div>
            ))}
          </div>
        )}
      </Panel>

      <Panel
        title="Active runtime config"
        subtitle="These values are loaded from /admin/config."
      >
        {config ? (
          <div className="grid gap-4 md:grid-cols-2">
            <div>
              <div className="label">Bind</div>
              <div className="mt-2 text-sm text-slate-700">
                {config.host}:{config.port}
              </div>
            </div>
            <div>
              <div className="label">DSN</div>
              <div className="mt-2 text-sm text-slate-700 break-all">{config.dsn}</div>
            </div>
            <div>
              <div className="label">Proxy</div>
              <div className="mt-2 text-sm text-slate-700">{config.proxy || "-"}</div>
            </div>
            <div>
              <div className="label">Data dir</div>
              <div className="mt-2 text-sm text-slate-700 break-all">
                {config.data_dir || "-"}
              </div>
            </div>
            <div>
              <div className="label">Config updated</div>
              <div className="mt-2 text-sm text-slate-700">
                {formatTimestamp(configUpdatedAt ?? undefined)}
              </div>
            </div>
          </div>
        ) : (
          <div className="text-sm text-slate-500">No config loaded.</div>
        )}
      </Panel>
    </div>
  );
}

import React, { useCallback, useEffect, useState } from "react";
import Panel from "../components/Panel";
import { apiErrorMessage, apiRequest } from "../lib/api";
import type { ProviderStats } from "../lib/types";

export default function StatsSection({
  adminKey,
  notify
}: {
  adminKey: string;
  notify: (toast: { type: "success" | "error" | "info" | "warning"; message: string }) => void;
}) {
  const [stats, setStats] = useState<ProviderStats[]>([]);
  const [loading, setLoading] = useState(false);

  const loadStats = useCallback(async () => {
    setLoading(true);
    try {
      const data = await apiRequest<{ providers: ProviderStats[] }>("/admin/stats", { adminKey });
      setStats(data.providers ?? []);
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    } finally {
      setLoading(false);
    }
  }, [adminKey, notify]);

  useEffect(() => {
    loadStats();
  }, [loadStats]);

  return (
    <Panel
      title="Provider stats"
      subtitle="Snapshot of provider pool health and credential counts."
      action={
        <button className="btn btn-ghost" type="button" onClick={loadStats}>
          Refresh
        </button>
      }
    >
      {loading ? (
        <div className="text-sm text-slate-500">Loading stats...</div>
      ) : stats.length === 0 ? (
        <div className="text-sm text-slate-500">No stats available.</div>
      ) : (
        <div className="overflow-hidden rounded-2xl border border-slate-200">
          <table className="w-full text-left text-sm">
            <thead className="bg-slate-50 text-xs uppercase text-slate-400">
              <tr>
                <th className="px-4 py-3">Provider</th>
                <th className="px-4 py-3">Total</th>
                <th className="px-4 py-3">Enabled</th>
                <th className="px-4 py-3">Disallow</th>
              </tr>
            </thead>
            <tbody>
              {stats.map((row) => (
                <tr key={row.name} className="border-t border-slate-200 bg-white/80">
                  <td className="px-4 py-3 font-semibold text-slate-800">{row.name}</td>
                  <td className="px-4 py-3 text-slate-600">{row.credentials_total}</td>
                  <td className="px-4 py-3 text-slate-600">{row.credentials_enabled}</td>
                  <td className="px-4 py-3 text-slate-600">{row.disallow}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </Panel>
  );
}

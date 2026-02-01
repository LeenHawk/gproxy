import React, { useCallback, useEffect, useMemo, useState } from "react";
import Panel from "../components/Panel";
import JsonBlock from "../components/JsonBlock";
import { apiErrorMessage, apiRequest } from "../lib/api";
import { formatTimestamp, toEpochSeconds } from "../lib/format";
import type { Credential, Provider, UpstreamUsage } from "../lib/types";

type FormState = {
  credentialId: string;
  model: string;
  start: string;
  end: string;
};

const emptyForm: FormState = {
  credentialId: "",
  model: "",
  start: "",
  end: ""
};

export default function UsageSection({
  adminKey,
  notify
}: {
  adminKey: string;
  notify: (toast: { type: "success" | "error" | "info" | "warning"; message: string }) => void;
}) {
  const [form, setForm] = useState<FormState>(emptyForm);
  const [credentials, setCredentials] = useState<Credential[]>([]);
  const [providers, setProviders] = useState<Provider[]>([]);
  const [result, setResult] = useState<UpstreamUsage | null>(null);
  const [loading, setLoading] = useState(false);
  const [showRaw, setShowRaw] = useState(false);

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

  const loadProviders = useCallback(async () => {
    try {
      const data = await apiRequest<Provider[]>("/admin/providers", { adminKey });
      const items = Array.isArray(data) ? data : [];
      setProviders(items);
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    }
  }, [adminKey, notify]);

  useEffect(() => {
    loadCredentials();
    loadProviders();
  }, [loadCredentials, loadProviders]);

  const providerMap = useMemo(() => {
    const map = new Map<number, Provider>();
    providers.forEach((provider) => map.set(provider.id, provider));
    return map;
  }, [providers]);

  const selectedCredential = useMemo(() => {
    if (!form.credentialId) {
      return null;
    }
    const id = Number(form.credentialId);
    return credentials.find((cred) => cred.id === id) ?? null;
  }, [credentials, form.credentialId]);

  const providerName = selectedCredential
    ? providerMap.get(selectedCredential.provider_id)?.name ?? "Unknown"
    : "Unknown";

  const tokenGroups = useMemo(
    () => [
      {
        title: "OpenAI tokens",
        keys: [
          "openai_chat_prompt_tokens",
          "openai_chat_completion_tokens",
          "openai_chat_total_tokens",
          "openai_responses_input_tokens",
          "openai_responses_output_tokens",
          "openai_responses_total_tokens",
          "openai_responses_input_cached_tokens",
          "openai_responses_output_reasoning_tokens"
        ]
      },
      {
        title: "Claude tokens",
        keys: [
          "claude_input_tokens",
          "claude_output_tokens",
          "claude_total_tokens",
          "claude_cache_creation_input_tokens",
          "claude_cache_read_input_tokens"
        ]
      },
      {
        title: "Gemini tokens",
        keys: [
          "gemini_prompt_tokens",
          "gemini_candidates_tokens",
          "gemini_total_tokens",
          "gemini_cached_tokens"
        ]
      }
    ],
    []
  );

  const tokenLabels: Record<string, string> = {
    openai_chat_prompt_tokens: "OpenAI chat prompt",
    openai_chat_completion_tokens: "OpenAI chat completion",
    openai_chat_total_tokens: "OpenAI chat total",
    openai_responses_input_tokens: "OpenAI responses input",
    openai_responses_output_tokens: "OpenAI responses output",
    openai_responses_total_tokens: "OpenAI responses total",
    openai_responses_input_cached_tokens: "OpenAI responses input cached",
    openai_responses_output_reasoning_tokens: "OpenAI responses reasoning",
    claude_input_tokens: "Claude input",
    claude_output_tokens: "Claude output",
    claude_total_tokens: "Claude total",
    claude_cache_creation_input_tokens: "Claude cache creation input",
    claude_cache_read_input_tokens: "Claude cache read input",
    gemini_prompt_tokens: "Gemini prompt",
    gemini_candidates_tokens: "Gemini candidates",
    gemini_total_tokens: "Gemini total",
    gemini_cached_tokens: "Gemini cached"
  };

  const tokenSummary = useMemo(() => {
    if (!result) {
      return null;
    }
    const tokens = result.tokens ?? {};
    const openaiTotal = Math.max(
      tokens.openai_chat_total_tokens ?? 0,
      tokens.openai_responses_total_tokens ?? 0
    );
    const claudeTotal =
      tokens.claude_total_tokens ??
      (tokens.claude_input_tokens ?? 0) + (tokens.claude_output_tokens ?? 0);
    const geminiTotal =
      tokens.gemini_total_tokens ??
      (tokens.gemini_prompt_tokens ?? 0) + (tokens.gemini_candidates_tokens ?? 0);
    return {
      openaiTotal,
      claudeTotal,
      geminiTotal
    };
  }, [result]);

  const handleSubmit = async () => {
    try {
      if (!form.credentialId) {
        throw new Error("Credential is required.");
      }
      const start = toEpochSeconds(form.start);
      const end = toEpochSeconds(form.end);
      if (start === null || end === null) {
        throw new Error("Start and end time are required.");
      }
      const params = new URLSearchParams();
      params.set("credential_id", form.credentialId);
      if (form.model.trim()) {
        params.set("model", form.model.trim());
      }
      params.set("start", String(start));
      params.set("end", String(end));
      setLoading(true);
      const data = await apiRequest<UpstreamUsage>(`/admin/upstream_usage?${params.toString()}`,
        { adminKey }
      );
      setResult(data);
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="space-y-6">
      <Panel title="Usage lookup" subtitle="Query upstream usage by credential and time window.">
        <div className="grid gap-4 md:grid-cols-2">
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
            <label className="label">Model (optional)</label>
            <input
              className="input"
              value={form.model}
              onChange={(event) => setForm((prev) => ({ ...prev, model: event.target.value }))}
              placeholder="gpt-4.1"
            />
          </div>
          <div>
            <label className="label">Start</label>
            <input
              className="input"
              type="datetime-local"
              value={form.start}
              onChange={(event) => setForm((prev) => ({ ...prev, start: event.target.value }))}
            />
          </div>
          <div>
            <label className="label">End</label>
            <input
              className="input"
              type="datetime-local"
              value={form.end}
              onChange={(event) => setForm((prev) => ({ ...prev, end: event.target.value }))}
            />
          </div>
        </div>
        <button className="btn btn-primary" type="button" onClick={handleSubmit}>
          Fetch usage
        </button>
      </Panel>

      <Panel title="Usage result" subtitle="Aggregated token and count totals.">
        {loading ? (
          <div className="text-sm text-slate-500">Fetching usage...</div>
        ) : result ? (
          <div className="space-y-4">
            <div className="grid gap-3 md:grid-cols-2">
              <div className="rounded-xl border border-slate-200 bg-white/90 p-3">
                <div className="text-xs uppercase text-slate-400">Credential</div>
                <div className="text-sm font-semibold text-slate-800">#{result.credential_id}</div>
                <div className="mt-1 text-xs text-slate-500">{providerName}</div>
              </div>
              <div className="rounded-xl border border-slate-200 bg-white/90 p-3">
                <div className="text-xs uppercase text-slate-400">Model</div>
                <div className="text-sm font-semibold text-slate-800">{result.model || "all"}</div>
              </div>
              <div className="rounded-xl border border-slate-200 bg-white/90 p-3">
                <div className="text-xs uppercase text-slate-400">Count</div>
                <div className="text-sm font-semibold text-slate-800">{result.count}</div>
              </div>
              <div className="rounded-xl border border-slate-200 bg-white/90 p-3">
                <div className="text-xs uppercase text-slate-400">Window</div>
                <div className="text-sm font-semibold text-slate-800">
                  {formatTimestamp(result.start)} → {formatTimestamp(result.end)}
                </div>
              </div>
            </div>
            {tokenSummary ? (
              <div className="grid gap-3 md:grid-cols-3">
                <div className="rounded-xl border border-slate-200 bg-white/90 p-3">
                  <div className="text-xs uppercase text-slate-400">OpenAI total</div>
                  <div className="text-sm font-semibold text-slate-800">{tokenSummary.openaiTotal}</div>
                </div>
                <div className="rounded-xl border border-slate-200 bg-white/90 p-3">
                  <div className="text-xs uppercase text-slate-400">Claude total</div>
                  <div className="text-sm font-semibold text-slate-800">{tokenSummary.claudeTotal}</div>
                </div>
                <div className="rounded-xl border border-slate-200 bg-white/90 p-3">
                  <div className="text-xs uppercase text-slate-400">Gemini total</div>
                  <div className="text-sm font-semibold text-slate-800">{tokenSummary.geminiTotal}</div>
                </div>
              </div>
            ) : null}

            <div className="space-y-3">
              {tokenGroups.map((group) => {
                const rows = group.keys
                  .map((key) => ({
                    key,
                    value: Number(result.tokens?.[key as keyof typeof result.tokens] ?? 0)
                  }))
                  .filter((row) => row.value > 0);
                if (rows.length === 0) {
                  return null;
                }
                return (
                  <div key={group.title} className="rounded-2xl border border-slate-200 bg-white/90">
                    <div className="border-b border-slate-200 px-4 py-3 text-sm font-semibold text-slate-700">
                      {group.title}
                    </div>
                    <div className="overflow-hidden">
                      <table className="w-full text-left text-sm">
                        <thead className="bg-slate-50 text-xs uppercase text-slate-400">
                          <tr>
                            <th className="px-4 py-2">Metric</th>
                            <th className="px-4 py-2 text-right">Value</th>
                          </tr>
                        </thead>
                        <tbody>
                          {rows.map((row) => (
                            <tr key={row.key} className="border-t border-slate-200">
                              <td className="px-4 py-2 text-slate-600">
                                {tokenLabels[row.key] ?? row.key}
                              </td>
                              <td className="px-4 py-2 text-right text-slate-800">
                                {row.value}
                              </td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  </div>
                );
              })}
            </div>

            <div>
              <button
                className="btn btn-ghost"
                type="button"
                onClick={() => setShowRaw((prev) => !prev)}
              >
                {showRaw ? "Hide raw JSON" : "Show raw JSON"}
              </button>
              {showRaw ? (
                <div className="mt-3">
                  <JsonBlock value={result.tokens} />
                </div>
              ) : null}
            </div>
          </div>
        ) : (
          <div className="text-sm text-slate-500">No usage queried yet.</div>
        )}
      </Panel>
    </div>
  );
}

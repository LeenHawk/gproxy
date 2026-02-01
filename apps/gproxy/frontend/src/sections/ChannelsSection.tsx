import React, { useCallback, useEffect, useMemo, useState } from "react";
import Panel from "../components/Panel";
import JsonBlock from "../components/JsonBlock";
import { apiErrorMessage, apiRequest } from "../lib/api";
import { formatTimestamp, maskValue } from "../lib/format";
import type { Credential, Provider } from "../lib/types";

type ProviderFormState = {
  name: string;
  enabled: boolean;
  configText: string;
};

type CredentialFormState = {
  name: string;
  weight: string;
  enabled: boolean;
  secretText: string;
};

type OauthField = "redirect_uri" | "base_url" | "project_id" | "scope" | "setup";

type ProviderProfile = {
  kind: "key" | "json" | "claudecode";
  keyField?: string;
  linePlaceholder?: string;
  template?: unknown;
  hint?: string;
  oauth?: {
    fields: OauthField[];
  };
};

type InputMode = "lines" | "json" | "session";

type OauthParams = {
  redirect_uri: string;
  base_url: string;
  project_id: string;
  scope: string;
  setup: boolean;
};

const emptyProviderForm: ProviderFormState = {
  name: "",
  enabled: true,
  configText: "{}"
};

const emptyCredentialForm: CredentialFormState = {
  name: "",
  weight: "1",
  enabled: true,
  secretText: "{}"
};

const defaultOauthParams: OauthParams = {
  redirect_uri: "",
  base_url: "",
  project_id: "",
  scope: "",
  setup: false
};

const UPSTREAM_USAGE_PROVIDERS = ["codex", "claudecode", "antigravity"] as const;

const formatDuration = (seconds?: number | null) => {
  if (!seconds || !Number.isFinite(seconds)) {
    return "-";
  }
  const total = Math.max(0, Math.floor(seconds));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  return hours > 0 ? `${hours}h ${minutes}m` : `${minutes}m`;
};

const formatLocalTime = (value?: number | string | null) => {
  if (!value) {
    return "-";
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return "-";
  }
  return date.toLocaleString();
};

const PROVIDER_PROFILES: Record<string, ProviderProfile> = {
  openai: {
    kind: "key",
    keyField: "key",
    linePlaceholder: "sk-... (one per line)",
    template: { key: "sk-..." }
  },
  claude: {
    kind: "key",
    keyField: "key",
    linePlaceholder: "sk-ant-... (one per line)",
    template: { key: "sk-ant-..." }
  },
  aistudio: {
    kind: "key",
    keyField: "key",
    linePlaceholder: "AIza... (one per line)",
    template: { key: "AIza..." }
  },
  vertexexpress: {
    kind: "key",
    keyField: "key",
    linePlaceholder: "vx-... (one per line)",
    template: { key: "vx-..." }
  },
  nvidia: {
    kind: "key",
    keyField: "key",
    linePlaceholder: "nvapi-... (one per line)",
    template: { key: "nvapi-..." }
  },
  deepseek: {
    kind: "key",
    keyField: "key",
    linePlaceholder: "deepseek-... (one per line)",
    template: { key: "deepseek-..." }
  },
  vertex: {
    kind: "json",
    template: {
      project_id: "your-gcp-project",
      client_email: "service-account@project.iam.gserviceaccount.com",
      private_key: "-----BEGIN PRIVATE KEY-----\\n...\\n-----END PRIVATE KEY-----"
    },
    hint: "Paste the GCP service account JSON fields you need."
  },
  geminicli: {
    kind: "json",
    template: {
      project_id: "your-gcp-project",
      client_email: "service-account@project.iam.gserviceaccount.com",
      private_key: "-----BEGIN PRIVATE KEY-----\\n...\\n-----END PRIVATE KEY-----"
    },
    hint: "Gemini CLI uses Google service account credentials.",
    oauth: {
      fields: ["redirect_uri", "base_url", "project_id"]
    }
  },
  antigravity: {
    kind: "json",
    template: {
      project_id: "your-gcp-project",
      client_email: "service-account@project.iam.gserviceaccount.com",
      private_key: "-----BEGIN PRIVATE KEY-----\\n...\\n-----END PRIVATE KEY-----"
    },
    hint: "Antigravity uses Google service account credentials.",
    oauth: {
      fields: ["redirect_uri", "base_url", "project_id"]
    }
  },
  codex: {
    kind: "json",
    template: {
      account_id: "acct_...",
      refresh_token: "rt_..."
    },
    hint: "Codex refresh tokens can be rotated from the Codex console.",
    oauth: {
      fields: ["redirect_uri"]
    }
  },
  claudecode: {
    kind: "claudecode",
    linePlaceholder: "sessionKey=... (one per line)",
    template: {
      sessionKey: "sessionKey=...",
      claudeAiOauth: {
        refreshToken: "...",
        accessToken: "...",
        expiresAt: 0
      }
    },
    hint: "Claude Code supports sessionKey or OAuth JSON payloads.",
    oauth: {
      fields: ["redirect_uri", "scope", "setup"]
    }
  }
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

function ensureProviderId(payload: unknown, providerId: number): unknown {
  if (!payload || typeof payload !== "object") {
    return payload;
  }
  const record = payload as Record<string, unknown>;
  if (record.provider_id !== undefined || record.provider_name !== undefined) {
    return payload;
  }
  return { ...record, provider_id: providerId };
}

function normalizeProviderName(name: string) {
  return name.toLowerCase().replace(/\s+/g, "");
}

function supportsUpstreamUsage(name?: string | null) {
  if (!name) {
    return false;
  }
  const key = normalizeProviderName(name);
  return UPSTREAM_USAGE_PROVIDERS.some((provider) => key.includes(provider));
}

function profileForProvider(name?: string): ProviderProfile {
  if (!name) {
    return { kind: "json", template: {} };
  }
  const key = normalizeProviderName(name);
  if (key.includes("claudecode")) {
    return PROVIDER_PROFILES.claudecode;
  }
  if (key.includes("openai")) {
    return PROVIDER_PROFILES.openai;
  }
  if (key.includes("claude")) {
    return PROVIDER_PROFILES.claude;
  }
  if (key.includes("aistudio")) {
    return PROVIDER_PROFILES.aistudio;
  }
  if (key.includes("vertexexpress")) {
    return PROVIDER_PROFILES.vertexexpress;
  }
  if (key.includes("vertex")) {
    return PROVIDER_PROFILES.vertex;
  }
  if (key.includes("geminicli")) {
    return PROVIDER_PROFILES.geminicli;
  }
  if (key.includes("antigravity")) {
    return PROVIDER_PROFILES.antigravity;
  }
  if (key.includes("codex")) {
    return PROVIDER_PROFILES.codex;
  }
  if (key.includes("nvidia")) {
    return PROVIDER_PROFILES.nvidia;
  }
  if (key.includes("deepseek")) {
    return PROVIDER_PROFILES.deepseek;
  }
  return { kind: "json", template: {} };
}

function renderUpstreamUsage(providerKey: string, payload: unknown) {
  if (!payload || typeof payload !== "object") {
    return <div className="text-sm text-slate-500">No upstream usage data.</div>;
  }
  if (providerKey.includes("codex")) {
    const data = payload as Record<string, any>;
    const primary = data.primary_window ?? {};
    const secondary = data.secondary_window ?? {};
    return (
      <div className="space-y-4">
        <div className="grid gap-3 md:grid-cols-2">
          <div className="rounded-xl border border-slate-200 bg-white/90 p-3">
            <div className="text-xs uppercase text-slate-400">Plan</div>
            <div className="text-sm font-semibold text-slate-800">{data.plan_type ?? "-"}</div>
          </div>
          <div className="rounded-xl border border-slate-200 bg-white/90 p-3">
            <div className="text-xs uppercase text-slate-400">Primary usage</div>
            <div className="text-sm font-semibold text-slate-800">
              {primary.used_percent ?? "-"}%
            </div>
            <div className="mt-1 text-xs text-slate-500">
              Reset at {formatTimestamp(primary.reset_at)}
            </div>
          </div>
        </div>
        <div className="grid gap-3 md:grid-cols-2">
          <div className="rounded-xl border border-slate-200 bg-white/90 p-3">
            <div className="text-xs uppercase text-slate-400">Primary window</div>
            <div className="text-sm font-semibold text-slate-800">
              {formatDuration(primary.reset_after_seconds)}
            </div>
            <div className="mt-1 text-xs text-slate-500">
              Window {primary.limit_window_seconds ?? "-"}s
            </div>
          </div>
          <div className="rounded-xl border border-slate-200 bg-white/90 p-3">
            <div className="text-xs uppercase text-slate-400">Secondary window</div>
            <div className="text-sm font-semibold text-slate-800">
              {secondary.used_percent ?? "-"}%
            </div>
            <div className="mt-1 text-xs text-slate-500">
              Reset at {formatTimestamp(secondary.reset_at)}
            </div>
          </div>
        </div>
      </div>
    );
  }
  if (providerKey.includes("claudecode")) {
    const data = payload as Record<string, any>;
    const windows = [
      { key: "five_hour", label: "5h window" },
      { key: "seven_day", label: "7d window" },
      { key: "seven_day_sonnet", label: "7d sonnet window" }
    ];
    const available = windows.filter((item) => data[item.key]);
    if (available.length === 0) {
      return (
        <div className="text-sm text-slate-500">No structured Claude Code usage fields.</div>
      );
    }
    return (
      <div className="grid gap-3 md:grid-cols-2">
        {available.map((item) => {
          const entry = data[item.key] ?? {};
          return (
            <div key={item.key} className="rounded-xl border border-slate-200 bg-white/90 p-3">
              <div className="text-xs uppercase text-slate-400">{item.label}</div>
              <div className="text-sm font-semibold text-slate-800">
                {entry.utilization ?? "-"}%
              </div>
              <div className="mt-1 text-xs text-slate-500">
                Resets at {formatLocalTime(entry.resets_at)}
              </div>
            </div>
          );
        })}
      </div>
    );
  }
  if (providerKey.includes("antigravity")) {
    const data = payload as Record<string, any>;
    const models = data.models ?? data.model_usage ?? {};
    const entries = Array.isArray(models) ? models : Object.entries(models as Record<string, any>);
    if (!entries.length) {
      return <div className="text-sm text-slate-500">No model usage available.</div>;
    }
    return (
      <div className="grid gap-3 md:grid-cols-2">
        {entries.map((entry: any) => {
          const [name, info] = Array.isArray(entry) ? entry : [entry.name, entry];
          const remaining =
            info?.remainingFraction ??
            info?.remaining_fraction ??
            info?.remaining ??
            "-";
          const reset = info?.resetTime ?? info?.reset_time ?? info?.reset ?? "-";
          return (
            <div key={String(name)} className="rounded-xl border border-slate-200 bg-white/90 p-3">
              <div className="text-xs uppercase text-slate-400">{name}</div>
              <div className="text-sm font-semibold text-slate-800">
                Remaining {remaining}
              </div>
              <div className="mt-1 text-xs text-slate-500">
                Reset {formatLocalTime(reset)}
              </div>
            </div>
          );
        })}
      </div>
    );
  }
  return <div className="text-sm text-slate-500">No upstream usage renderer.</div>;
}

function parseCallbackInput(input: string): { state?: string; code?: string; error?: string } {
  if (!input.trim()) {
    return { error: "Callback input is empty." };
  }
  let query = input.trim();
  if (query.includes("http://") || query.includes("https://")) {
    try {
      const url = new URL(query);
      query = url.search.startsWith("?") ? url.search.slice(1) : url.search;
    } catch {
      return { error: "Invalid URL in callback input." };
    }
  } else if (query.includes("?")) {
    query = query.split("?").slice(1).join("?");
  }
  const params = new URLSearchParams(query);
  const state = params.get("state") || undefined;
  const code = params.get("code") || undefined;
  if (!state || !code) {
    return { error: "Missing state or code in callback input." };
  }
  return { state, code };
}

function hasMeta(value: unknown): boolean {
  if (value === null || value === undefined) {
    return false;
  }
  if (Array.isArray(value)) {
    return value.length > 0;
  }
  if (typeof value === "object") {
    return Object.keys(value as Record<string, unknown>).length > 0;
  }
  return true;
}

function parseCredentialInput(text: string): { secret: unknown; meta_json: unknown } {
  const parsed = parseJson(text, {});
  if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
    const record = parsed as Record<string, unknown>;
    if ("secret" in record || "meta" in record || "meta_json" in record) {
      return {
        secret: record.secret ?? {},
        meta_json: record.meta ?? record.meta_json ?? {}
      };
    }
  }
  return { secret: parsed, meta_json: {} };
}

function composeCredentialJson(credential: Credential): string {
  if (hasMeta(credential.meta_json)) {
    return JSON.stringify({ secret: credential.secret ?? {}, meta: credential.meta_json }, null, 2);
  }
  return JSON.stringify(credential.secret ?? {}, null, 2);
}

function normalizeLineValue(value: string, keyField: string) {
  const trimmed = value.trim();
  const lower = trimmed.toLowerCase();
  const keyLower = keyField.toLowerCase();
  if (lower.startsWith(`${keyLower}=`)) {
    return trimmed.slice(keyLower.length + 1);
  }
  if (keyLower === "sessionkey" && lower.startsWith("session_key=")) {
    return trimmed.slice("session_key=".length);
  }
  if (keyLower === "key" && lower.startsWith("apikey=")) {
    return trimmed.slice("apikey=".length);
  }
  return trimmed;
}

export default function ChannelsSection({
  adminKey,
  notify
}: {
  adminKey: string;
  notify: (toast: { type: "success" | "error" | "info" | "warning"; message: string }) => void;
}) {
  const [providers, setProviders] = useState<Provider[]>([]);
  const [credentials, setCredentials] = useState<Credential[]>([]);
  const [selectedProviderId, setSelectedProviderId] = useState<number | null>(null);
  const [providerForm, setProviderForm] = useState<ProviderFormState>(emptyProviderForm);
  const [editingProviderId, setEditingProviderId] = useState<number | null>(null);
  const [credentialForm, setCredentialForm] = useState<CredentialFormState>(emptyCredentialForm);
  const [editingCredentialId, setEditingCredentialId] = useState<number | null>(null);
  const [expanded, setExpanded] = useState<Record<number, boolean>>({});
  const [usageByCredential, setUsageByCredential] = useState<Record<number, unknown>>({});
  const [usageLoading, setUsageLoading] = useState<Record<number, boolean>>({});
  const [usageErrors, setUsageErrors] = useState<Record<number, string>>({});
  const [usageRaw, setUsageRaw] = useState<Record<number, boolean>>({});
  const [loadingProviders, setLoadingProviders] = useState(false);
  const [loadingCredentials, setLoadingCredentials] = useState(false);
  const [batchText, setBatchText] = useState("");
  const [batchFiles, setBatchFiles] = useState<File[]>([]);
  const [lineInput, setLineInput] = useState("");
  const [inputMode, setInputMode] = useState<InputMode>("json");
  const [oauthParams, setOauthParams] = useState<OauthParams>(defaultOauthParams);
  const [oauthUrl, setOauthUrl] = useState("");
  const [oauthStateId, setOauthStateId] = useState("");
  const [oauthCallbackInput, setOauthCallbackInput] = useState("");
  const [oauthBusy, setOauthBusy] = useState(false);

  const loadProviders = useCallback(async () => {
    setLoadingProviders(true);
    try {
      const data = await apiRequest<Provider[]>("/admin/providers", { adminKey });
      const items = Array.isArray(data) ? data : [];
      setProviders(items);
      if (items.length > 0) {
        const exists = selectedProviderId !== null && items.some((item) => item.id === selectedProviderId);
        if (!exists) {
          setSelectedProviderId(items[0].id);
        }
      }
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    } finally {
      setLoadingProviders(false);
    }
  }, [adminKey, notify, selectedProviderId]);

  const loadCredentials = useCallback(async () => {
    setLoadingCredentials(true);
    try {
      const data = await apiRequest<Credential[]>("/admin/credentials", { adminKey });
      setCredentials(Array.isArray(data) ? data : []);
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    } finally {
      setLoadingCredentials(false);
    }
  }, [adminKey, notify]);

  useEffect(() => {
    loadProviders();
    loadCredentials();
  }, [loadProviders, loadCredentials]);

  const selectedProvider = useMemo(
    () => providers.find((provider) => provider.id === selectedProviderId) ?? null,
    [providers, selectedProviderId]
  );

  const providerProfile = useMemo(
    () => profileForProvider(selectedProvider?.name),
    [selectedProvider?.name]
  );

  const providerUsageKey = useMemo(
    () => (selectedProvider?.name ? normalizeProviderName(selectedProvider.name) : ""),
    [selectedProvider?.name]
  );

  const providerSupportsUsage = useMemo(
    () => supportsUpstreamUsage(selectedProvider?.name),
    [selectedProvider?.name]
  );

  const modeOptions = useMemo(() => {
    if (providerProfile.kind === "claudecode") {
      return ["session", "json"] as InputMode[];
    }
    if (providerProfile.kind === "key") {
      return ["lines", "json"] as InputMode[];
    }
    return ["json"] as InputMode[];
  }, [providerProfile.kind]);

  useEffect(() => {
    if (!selectedProvider) {
      setProviderForm(emptyProviderForm);
      setEditingProviderId(null);
      return;
    }
    setProviderForm({
      name: selectedProvider.name,
      enabled: selectedProvider.enabled,
      configText: JSON.stringify(selectedProvider.config_json ?? {}, null, 2)
    });
    setEditingProviderId(selectedProvider.id);
    setEditingCredentialId(null);
    setLineInput("");
    setInputMode(modeOptions[0]);
    setOauthParams(defaultOauthParams);
    setOauthUrl("");
    setOauthStateId("");
    setOauthCallbackInput("");
    if (modeOptions[0] === "json") {
      const template = providerProfile.template ?? {};
      setCredentialForm({
        ...emptyCredentialForm,
        secretText: JSON.stringify(template, null, 2)
      });
    } else {
      setCredentialForm({ ...emptyCredentialForm, secretText: "{}" });
    }
  }, [selectedProvider, modeOptions, providerProfile.template]);

  useEffect(() => {
    if (inputMode !== "json" || editingCredentialId !== null) {
      return;
    }
    const template = providerProfile.template ?? {};
    setCredentialForm((prev) => {
      const current = prev.secretText.trim();
      if (current && current !== "{}") {
        return prev;
      }
      return {
        ...prev,
        secretText: JSON.stringify(template, null, 2)
      };
    });
  }, [inputMode, providerProfile.template, editingCredentialId]);

  const providerCredentials = useMemo(() => {
    if (!selectedProviderId) {
      return [];
    }
    return credentials.filter((cred) => cred.provider_id === selectedProviderId);
  }, [credentials, selectedProviderId]);

  const resetProviderForm = () => {
    setProviderForm(emptyProviderForm);
    setEditingProviderId(null);
  };

  const handleProviderSave = async () => {
    try {
      const payload = {
        id: editingProviderId ?? undefined,
        name: providerForm.name.trim(),
        enabled: providerForm.enabled,
        config_json: parseJson(providerForm.configText, {})
      };
      if (!payload.name) {
        throw new Error("Provider name is required.");
      }
      if (editingProviderId) {
        await apiRequest(`/admin/providers/${editingProviderId}`, {
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
      resetProviderForm();
      await loadProviders();
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    }
  };

  const handleProviderDelete = async (provider: Provider) => {
    if (!confirm(`Delete provider ${provider.name}?`)) {
      return;
    }
    try {
      await apiRequest(`/admin/providers/${provider.id}`, { method: "DELETE", adminKey });
      notify({ type: "success", message: "Provider deleted." });
      if (selectedProviderId === provider.id) {
        setSelectedProviderId(null);
      }
      await loadProviders();
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    }
  };

  const resetCredentialForm = () => {
    setCredentialForm(emptyCredentialForm);
    setEditingCredentialId(null);
  };

  const buildPayload = (secret: unknown, meta_json: unknown = {}) => ({
    id: editingCredentialId ?? undefined,
    provider_id: selectedProviderId,
    name: credentialForm.name.trim() ? credentialForm.name.trim() : null,
    secret,
    meta_json,
    weight: Number(credentialForm.weight || 1),
    enabled: credentialForm.enabled
  });

  const handleCredentialSave = async () => {
    if (!selectedProviderId) {
      notify({ type: "error", message: "Select a provider first." });
      return;
    }
    try {
      if (inputMode === "lines" || inputMode === "session") {
        const lines = lineInput
          .split(/\r?\n/)
          .map((line) => line.trim())
          .filter(Boolean);
        if (lines.length === 0) {
          throw new Error("No keys provided.");
        }
        if (Number.isNaN(Number(credentialForm.weight || 1))) {
          throw new Error("Weight must be a number.");
        }
        const keyField = inputMode === "session" ? "sessionKey" : providerProfile.keyField ?? "key";
        for (const line of lines) {
          const value = normalizeLineValue(line, keyField);
          const payload = buildPayload({ [keyField]: value });
          await apiRequest("/admin/credentials", { method: "POST", body: payload, adminKey });
        }
        notify({ type: "success", message: `Uploaded ${lines.length} credential(s).` });
        setLineInput("");
      } else {
        const parsed = parseCredentialInput(credentialForm.secretText);
        const payload = buildPayload(parsed.secret, parsed.meta_json);
        if (Number.isNaN(payload.weight)) {
          throw new Error("Weight must be a number.");
        }
        if (editingCredentialId) {
          await apiRequest(`/admin/credentials/${editingCredentialId}`, {
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
        resetCredentialForm();
      }
      await loadCredentials();
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    }
  };

  const handleCredentialEdit = (credential: Credential) => {
    setEditingCredentialId(credential.id);
    setInputMode("json");
    setLineInput("");
    setCredentialForm({
      name: credential.name ?? "",
      weight: String(credential.weight ?? 1),
      enabled: credential.enabled,
      secretText: composeCredentialJson(credential)
    });
  };

  const handleCredentialDelete = async (credential: Credential) => {
    if (!confirm(`Delete credential ${credential.id}?`)) {
      return;
    }
    try {
      await apiRequest(`/admin/credentials/${credential.id}`, { method: "DELETE", adminKey });
      notify({ type: "success", message: "Credential deleted." });
      await loadCredentials();
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    }
  };

  const handleUsageFetch = async (credential: Credential) => {
    setUsageLoading((prev) => ({ ...prev, [credential.id]: true }));
    setUsageErrors((prev) => ({ ...prev, [credential.id]: "" }));
    try {
      const params = new URLSearchParams();
      params.set("credential_id", String(credential.id));
      const data = await apiRequest<unknown>(`/admin/upstream_usage_live?${params.toString()}`, {
        adminKey
      });
      setUsageByCredential((prev) => ({ ...prev, [credential.id]: data }));
      setUsageRaw((prev) => ({ ...prev, [credential.id]: false }));
    } catch (error) {
      const message = apiErrorMessage(error);
      setUsageErrors((prev) => ({ ...prev, [credential.id]: message }));
      notify({ type: "error", message });
    } finally {
      setUsageLoading((prev) => ({ ...prev, [credential.id]: false }));
    }
  };

  const handleBatchUpload = async () => {
    if (!selectedProviderId) {
      notify({ type: "error", message: "Select a provider first." });
      return;
    }
    try {
      const payloads: unknown[] = [];
      if (batchText.trim()) {
        const parsed = JSON.parse(batchText);
        if (Array.isArray(parsed)) {
          parsed.forEach((item) => payloads.push(ensureProviderId(item, selectedProviderId)));
        } else {
          payloads.push(ensureProviderId(parsed, selectedProviderId));
        }
      }
      for (const file of batchFiles) {
        const text = await readFileAsText(file);
        const parsed = JSON.parse(text);
        if (Array.isArray(parsed)) {
          parsed.forEach((item) => payloads.push(ensureProviderId(item, selectedProviderId)));
        } else {
          payloads.push(ensureProviderId(parsed, selectedProviderId));
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

  const insertTemplate = () => {
    const template = providerProfile.template ?? {};
    setCredentialForm((prev) => ({
      ...prev,
      secretText: JSON.stringify(template, null, 2)
    }));
  };

  const providerPath = selectedProvider ? normalizeProviderName(selectedProvider.name) : "";
  const oauthFields = providerProfile.oauth?.fields ?? [];

  const buildOauthParams = (state?: string, code?: string) => {
    const params = new URLSearchParams();
    if (state) {
      params.set("state", state);
    }
    if (code) {
      params.set("code", code);
    }
    if (oauthParams.redirect_uri.trim()) {
      params.set("redirect_uri", oauthParams.redirect_uri.trim());
    }
    if (oauthParams.base_url.trim()) {
      params.set("base_url", oauthParams.base_url.trim());
    }
    if (oauthParams.project_id.trim()) {
      params.set("project_id", oauthParams.project_id.trim());
    }
    if (oauthParams.scope.trim()) {
      params.set("scope", oauthParams.scope.trim());
    }
    if (oauthParams.setup) {
      params.set("setup", "true");
    }
    return params;
  };

  const handleOauthStart = async () => {
    if (!providerPath) {
      return;
    }
    setOauthBusy(true);
    try {
      const params = buildOauthParams();
      const query = params.toString();
      const data = await apiRequest<{ auth_url?: string; state?: string }>(
        `/${providerPath}/oauth${query ? `?${query}` : ""}`,
        { adminKey }
      );
      if (!data?.auth_url) {
        throw new Error("OAuth URL missing in response.");
      }
      setOauthUrl(data.auth_url);
      setOauthStateId(data.state ?? "");
      notify({ type: "success", message: "OAuth URL created." });
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    } finally {
      setOauthBusy(false);
    }
  };

  const handleOauthCallback = async () => {
    if (!providerPath) {
      return;
    }
    setOauthBusy(true);
    try {
      const parsed = parseCallbackInput(oauthCallbackInput);
      if (parsed.error) {
        throw new Error(parsed.error);
      }
      const params = buildOauthParams(parsed.state, parsed.code);
      await apiRequest(`/${providerPath}/oauth/callback?${params.toString()}`, { adminKey });
      notify({ type: "success", message: "OAuth callback accepted." });
      setOauthCallbackInput("");
      await loadCredentials();
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    } finally {
      setOauthBusy(false);
    }
  };

  return (
    <div className="grid gap-6 lg:grid-cols-[280px_1fr]">
      <Panel
        title="Channels"
        subtitle="Select a provider to manage its credentials."
        action={
          <button className="btn btn-ghost" type="button" onClick={loadProviders}>
            Refresh
          </button>
        }
      >
        {loadingProviders ? (
          <div className="text-sm text-slate-500">Loading providers...</div>
        ) : providers.length === 0 ? (
          <div className="text-sm text-slate-500">No providers registered.</div>
        ) : (
          <div className="space-y-2">
            {providers.map((provider) => (
              <button
                key={provider.id}
                type="button"
                onClick={() => setSelectedProviderId(provider.id)}
                className={`flex w-full items-center justify-between rounded-xl border px-3 py-2 text-left text-sm transition-all ${
                  provider.id === selectedProviderId
                    ? "border-slate-900 bg-slate-900 text-white"
                    : "border-slate-200 bg-white/80 text-slate-700 hover:bg-slate-50"
                }`}
              >
                <div>
                  <div className="font-semibold">{provider.name}</div>
                  <div className={`text-xs ${provider.id === selectedProviderId ? "text-slate-200" : "text-slate-400"}`}>
                    #{provider.id}
                  </div>
                </div>
                <span
                  className={`badge ${
                    provider.enabled
                      ? "border-emerald-200 text-emerald-600"
                      : "border-slate-200 text-slate-400"
                  }`}
                >
                  {provider.enabled ? "Enabled" : "Disabled"}
                </span>
              </button>
            ))}
          </div>
        )}
        <div className="mt-4">
          <button className="btn btn-primary w-full" type="button" onClick={resetProviderForm}>
            New provider
          </button>
        </div>
      </Panel>

      <div className="space-y-6">
        <Panel
          title={editingProviderId ? "Edit provider" : "Create provider"}
          subtitle="Provider config, status, and metadata."
        >
          <div className="grid gap-4 md:grid-cols-3">
            <div>
              <label className="label">Provider name</label>
              <input
                className="input"
                value={providerForm.name}
                onChange={(event) =>
                  setProviderForm((prev) => ({ ...prev, name: event.target.value }))
                }
              />
            </div>
            <div>
              <label className="label">Enabled</label>
              <select
                className="select"
                value={providerForm.enabled ? "true" : "false"}
                onChange={(event) =>
                  setProviderForm((prev) => ({ ...prev, enabled: event.target.value === "true" }))
                }
              >
                <option value="true">Enabled</option>
                <option value="false">Disabled</option>
              </select>
            </div>
            <div className="flex items-end gap-2">
              <button className="btn btn-primary w-full" type="button" onClick={handleProviderSave}>
                {editingProviderId ? "Save provider" : "Create provider"}
              </button>
              {editingProviderId && selectedProvider ? (
                <button
                  className="btn btn-danger"
                  type="button"
                  onClick={() => handleProviderDelete(selectedProvider)}
                >
                  Delete
                </button>
              ) : null}
            </div>
          </div>
          <div>
            <label className="label">Config JSON</label>
            <textarea
              className="textarea"
              rows={6}
              value={providerForm.configText}
              onChange={(event) =>
                setProviderForm((prev) => ({ ...prev, configText: event.target.value }))
              }
            />
          </div>
          {selectedProvider ? (
            <div className="text-xs text-slate-500">
              Updated {formatTimestamp(selectedProvider.updated_at)}
            </div>
          ) : null}
        </Panel>

        <Panel
          title="Credentials"
          subtitle={
            selectedProvider
              ? `Manage credentials for ${selectedProvider.name}.`
              : "Select a provider first."
          }
          action={
            <button className="btn btn-ghost" type="button" onClick={loadCredentials}>
              Refresh
            </button>
          }
        >
          {!selectedProvider ? (
            <div className="text-sm text-slate-500">Select a provider to continue.</div>
          ) : (
            <div className="space-y-6">
              {oauthFields.length > 0 ? (
                <div className="rounded-2xl border border-slate-200 bg-white/70 p-4">
                  <div className="flex flex-wrap items-center justify-between gap-3">
                    <div>
                      <div className="text-sm font-semibold text-slate-700">OAuth helper</div>
                      <div className="mt-1 text-xs text-slate-500">
                        Generate an auth URL, complete login, then paste the callback query here.
                      </div>
                    </div>
                    <button
                      className="btn btn-primary"
                      type="button"
                      onClick={handleOauthStart}
                      disabled={oauthBusy}
                    >
                      {oauthBusy ? "Working..." : "Create OAuth URL"}
                    </button>
                  </div>
                  <div className="mt-4 grid gap-3 md:grid-cols-2">
                    {oauthFields.includes("redirect_uri") ? (
                      <div>
                        <label className="label">redirect_uri</label>
                        <input
                          className="input"
                          value={oauthParams.redirect_uri}
                          onChange={(event) =>
                            setOauthParams((prev) => ({
                              ...prev,
                              redirect_uri: event.target.value
                            }))
                          }
                          placeholder="http://localhost:1455/auth/callback"
                        />
                      </div>
                    ) : null}
                    {oauthFields.includes("base_url") ? (
                      <div>
                        <label className="label">base_url</label>
                        <input
                          className="input"
                          value={oauthParams.base_url}
                          onChange={(event) =>
                            setOauthParams((prev) => ({
                              ...prev,
                              base_url: event.target.value
                            }))
                          }
                          placeholder="https://api.service.local"
                        />
                      </div>
                    ) : null}
                    {oauthFields.includes("project_id") ? (
                      <div>
                        <label className="label">project_id</label>
                        <input
                          className="input"
                          value={oauthParams.project_id}
                          onChange={(event) =>
                            setOauthParams((prev) => ({
                              ...prev,
                              project_id: event.target.value
                            }))
                          }
                          placeholder="gcp-project"
                        />
                      </div>
                    ) : null}
                    {oauthFields.includes("scope") ? (
                      <div>
                        <label className="label">scope</label>
                        <input
                          className="input"
                          value={oauthParams.scope}
                          onChange={(event) =>
                            setOauthParams((prev) => ({
                              ...prev,
                              scope: event.target.value
                            }))
                          }
                          placeholder="optional scope override"
                        />
                      </div>
                    ) : null}
                    {oauthFields.includes("setup") ? (
                      <label className="flex items-center gap-2 text-sm text-slate-600">
                        <input
                          type="checkbox"
                          checked={oauthParams.setup}
                          onChange={(event) =>
                            setOauthParams((prev) => ({
                              ...prev,
                              setup: event.target.checked
                            }))
                          }
                        />
                        setup
                      </label>
                    ) : null}
                  </div>
                  {oauthUrl ? (
                    <div className="mt-4 rounded-xl border border-slate-200 bg-white/80 p-3 text-sm">
                      <div className="text-xs uppercase text-slate-400">Auth URL</div>
                      <a
                        className="break-all text-sm text-sky-600 underline"
                        href={oauthUrl}
                        target="_blank"
                        rel="noreferrer"
                      >
                        {oauthUrl}
                      </a>
                      {oauthStateId ? (
                        <div className="mt-2 text-xs text-slate-500">state: {oauthStateId}</div>
                      ) : null}
                    </div>
                  ) : null}
                  <div className="mt-4">
                    <label className="label">Callback query or URL</label>
                    <textarea
                      className="textarea"
                      rows={3}
                      value={oauthCallbackInput}
                      onChange={(event) => setOauthCallbackInput(event.target.value)}
                      placeholder="state=...&code=..."
                    />
                    <button
                      className="btn btn-accent mt-3"
                      type="button"
                      onClick={handleOauthCallback}
                      disabled={oauthBusy}
                    >
                      Submit OAuth callback
                    </button>
                  </div>
                </div>
              ) : null}

              <div className="rounded-2xl border border-slate-200 bg-white/70 p-4">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div>
                    <div className="text-sm font-semibold text-slate-700">Add credential</div>
                    {providerProfile.hint ? (
                      <div className="mt-1 text-xs text-slate-500">{providerProfile.hint}</div>
                    ) : null}
                  </div>
                  {modeOptions.length > 1 ? (
                    <div className="flex flex-wrap gap-2">
                      {modeOptions.map((mode) => (
                        <button
                          key={mode}
                          type="button"
                          onClick={() => setInputMode(mode)}
                          disabled={editingCredentialId !== null}
                          className={`rounded-full border px-3 py-1 text-xs font-semibold ${
                            inputMode === mode
                              ? "border-slate-900 bg-slate-900 text-white"
                              : "border-slate-200 text-slate-500"
                          } ${editingCredentialId !== null ? "cursor-not-allowed opacity-60" : ""}`}
                        >
                          {mode === "lines" && "Keys"}
                          {mode === "session" && "Session key"}
                          {mode === "json" && "JSON"}
                        </button>
                      ))}
                    </div>
                  ) : null}
                </div>

                <div className="mt-4 grid gap-4 md:grid-cols-3">
                  <div>
                    <label className="label">Label</label>
                    <input
                      className="input"
                      value={credentialForm.name}
                      onChange={(event) =>
                        setCredentialForm((prev) => ({ ...prev, name: event.target.value }))
                      }
                      placeholder="Optional label"
                    />
                  </div>
                  <div>
                    <label className="label">Weight</label>
                    <input
                      className="input"
                      type="number"
                      min={0}
                      value={credentialForm.weight}
                      onChange={(event) =>
                        setCredentialForm((prev) => ({ ...prev, weight: event.target.value }))
                      }
                    />
                  </div>
                  <div className="flex items-end">
                    <button className="btn btn-primary w-full" type="button" onClick={handleCredentialSave}>
                      {editingCredentialId ? "Save credential" : "Add credential"}
                    </button>
                  </div>
                </div>

                {inputMode === "lines" || inputMode === "session" ? (
                  <div className="mt-4">
                    <label className="label">
                      {inputMode === "session" ? "Session keys (one per line)" : "Keys (one per line)"}
                    </label>
                    <textarea
                      className="textarea"
                      rows={5}
                      value={lineInput}
                      onChange={(event) => setLineInput(event.target.value)}
                      placeholder={
                        inputMode === "session"
                          ? providerProfile.linePlaceholder ?? "sessionKey=..."
                          : providerProfile.linePlaceholder ?? "sk-..."
                      }
                    />
                    <div className="mt-2 text-xs text-slate-400">
                      Each line creates a new credential under this provider.
                    </div>
                  </div>
                ) : (
                  <div className="mt-4">
                    <label className="label">Credential JSON</label>
                    <textarea
                      className="textarea"
                      rows={7}
                      value={credentialForm.secretText}
                      onChange={(event) =>
                        setCredentialForm((prev) => ({ ...prev, secretText: event.target.value }))
                      }
                    />
                    <div className="mt-2 flex flex-wrap items-center gap-3">
                      <button className="btn btn-ghost" type="button" onClick={insertTemplate}>
                        Insert template
                      </button>
                      <span className="text-xs text-slate-400">
                        You can include {"{ secret: { ... }, meta: { ... } }"} in the JSON if needed.
                      </span>
                    </div>
                  </div>
                )}

                <label className="mt-4 flex items-center gap-2 text-sm text-slate-600">
                  <input
                    type="checkbox"
                    checked={credentialForm.enabled}
                    onChange={(event) =>
                      setCredentialForm((prev) => ({ ...prev, enabled: event.target.checked }))
                    }
                  />
                  Enabled
                </label>
                {editingCredentialId ? (
                  <button className="btn btn-ghost mt-2" type="button" onClick={resetCredentialForm}>
                    Cancel edit
                  </button>
                ) : null}
              </div>

              {inputMode === "json" ? (
                <div className="rounded-2xl border border-slate-200 bg-white/70 p-4">
                  <div className="text-sm font-semibold text-slate-700">Batch JSON upload</div>
                  <div className="mt-3 grid gap-4 md:grid-cols-2">
                    <div>
                      <label className="label">JSON payloads</label>
                      <textarea
                        className="textarea"
                        rows={5}
                        value={batchText}
                        onChange={(event) => setBatchText(event.target.value)}
                        placeholder='[{"secret": {"key": "sk-..."}}]'
                      />
                    </div>
                    <div>
                      <label className="label">Files</label>
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
                          {batchFiles.length
                            ? `${batchFiles.length} file(s) selected.`
                            : "No files selected."}
                        </div>
                      </div>
                    </div>
                  </div>
                  <button className="btn btn-accent mt-3" type="button" onClick={handleBatchUpload}>
                    Upload JSON to {selectedProvider.name}
                  </button>
                </div>
              ) : null}

              <div className="rounded-2xl border border-slate-200 bg-white/70 p-4 text-xs text-slate-500">
                Upstream usage is available for codex, claudecode, and antigravity providers.
                {providerSupportsUsage
                  ? " Fetch it per credential below."
                  : " This provider does not expose upstream usage."}
              </div>

              {loadingCredentials ? (
                <div className="text-sm text-slate-500">Loading credentials...</div>
              ) : providerCredentials.length === 0 ? (
                <div className="text-sm text-slate-500">No credentials for this provider.</div>
              ) : (
                <div className="space-y-3">
                  {providerCredentials.map((credential) => (
                    <div
                      key={credential.id}
                      className="rounded-2xl border border-slate-200 bg-white/90 p-4"
                    >
                      <div className="flex flex-wrap items-start justify-between gap-3">
                        <div>
                          <div className="text-sm font-semibold text-slate-800">
                            {credential.name || `Credential #${credential.id}`}
                          </div>
                          <div className="mt-1 text-xs text-slate-400">
                            {maskValue(String(credential.id))}
                          </div>
                        </div>
                        <div className="flex flex-wrap items-center gap-2">
                          <span
                            className={`badge ${
                              credential.enabled
                                ? "border-emerald-200 text-emerald-600"
                                : "border-slate-200 text-slate-400"
                            }`}
                          >
                            {credential.enabled ? "Enabled" : "Disabled"}
                          </span>
                          <button
                            className="btn btn-ghost"
                            type="button"
                            onClick={() => handleCredentialEdit(credential)}
                          >
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
                            onClick={() => handleCredentialDelete(credential)}
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
                      {providerSupportsUsage ? (
                        <div className="mt-4 rounded-xl border border-slate-200 bg-white/80 p-3">
                          <div className="flex flex-wrap items-center justify-between gap-2">
                            <div className="text-xs uppercase text-slate-400">Upstream usage</div>
                            <button
                              className="btn btn-ghost"
                              type="button"
                              onClick={() => handleUsageFetch(credential)}
                              disabled={usageLoading[credential.id]}
                            >
                              {usageLoading[credential.id] ? "Fetching..." : "Fetch usage"}
                            </button>
                          </div>
                          <div className="mt-3">
                            {usageLoading[credential.id] ? (
                              <div className="text-sm text-slate-500">Fetching upstream usage...</div>
                            ) : usageErrors[credential.id] ? (
                              <div className="text-sm text-rose-600">{usageErrors[credential.id]}</div>
                            ) : usageByCredential[credential.id] ? (
                              renderUpstreamUsage(providerUsageKey, usageByCredential[credential.id])
                            ) : (
                              <div className="text-sm text-slate-500">No upstream usage fetched yet.</div>
                            )}
                          </div>
                          {usageByCredential[credential.id] ? (
                            <div className="mt-3">
                              <button
                                className="btn btn-ghost"
                                type="button"
                                onClick={() =>
                                  setUsageRaw((prev) => ({
                                    ...prev,
                                    [credential.id]: !prev[credential.id]
                                  }))
                                }
                              >
                                {usageRaw[credential.id] ? "Hide raw JSON" : "Show raw JSON"}
                              </button>
                              {usageRaw[credential.id] ? (
                                <div className="mt-3">
                                  <JsonBlock value={usageByCredential[credential.id]} />
                                </div>
                              ) : null}
                            </div>
                          ) : null}
                        </div>
                      ) : null}
                      {expanded[credential.id] ? (
                        <div className="mt-3">
                          <div className="label">Secret</div>
                          <JsonBlock value={credential.secret} />
                        </div>
                      ) : null}
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}
        </Panel>
      </div>
    </div>
  );
}

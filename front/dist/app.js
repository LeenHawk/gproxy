(() => {
  const { useEffect, useMemo, useState } = React;
  const html = htm.bind(React.createElement);

  const PROVIDERS = [
    { id: "openai", label: "OpenAI", keyFields: ["key"], oauth: false },
    { id: "claude", label: "Claude", keyFields: ["key"], oauth: false },
    { id: "claudecode", label: "Claude Code", keyFields: ["refresh_token"], oauth: true },
    { id: "codex", label: "Codex", keyFields: ["account_id", "refresh_token"], oauth: true, oauthParam: "workspace_id" },
    { id: "aistudio", label: "AI Studio", keyFields: ["key"], oauth: false },
    { id: "vertex", label: "Vertex", keyFields: ["project_id", "client_email"], oauth: false },
    { id: "vertexexpress", label: "Vertex Express", keyFields: ["key"], oauth: false },
    { id: "geminicli", label: "Gemini CLI", keyFields: ["project_id", "client_email"], oauth: true, oauthParam: "project_id" },
    { id: "antigravity", label: "Antigravity", keyFields: ["project_id", "client_email"], oauth: true, oauthParam: "project_id" },
    { id: "nvidia", label: "NVIDIA", keyFields: ["key"], oauth: false },
    { id: "deepseek", label: "DeepSeek", keyFields: ["key"], oauth: false }
  ];

  const CHANNELS = PROVIDERS;
  const BATCH_PROVIDERS = ["vertex", "codex", "claudecode", "geminicli", "antigravity"];

  const DEFAULT_TAB = "channels";

  const STATUS_COLORS = {
    active: "bg-emerald-100 text-emerald-700 border-emerald-200",
    disabled: "bg-slate-100 text-slate-600 border-slate-200",
    cooldown: "bg-amber-100 text-amber-700 border-amber-200",
    transient: "bg-sky-100 text-sky-700 border-sky-200"
  };

  const SPECIAL_USAGE_PROVIDERS = new Set(["codex", "claudecode", "antigravity"]);

  const usageCredentialId = (providerId, credential) => {
    if (!credential) {
      return "";
    }
    switch (providerId) {
      case "codex":
      case "claudecode":
        return credential.refresh_token || "";
      case "geminicli":
      case "antigravity":
      case "vertex":
        return credential.project_id || "";
      case "openai":
      case "claude":
      case "aistudio":
      case "nvidia":
      case "deepseek":
      case "vertexexpress":
        return credential.key || "";
      default:
        return credential.key || credential.project_id || "";
    }
  };

  const usageTotalTokens = (record) => {
    const totals = [
      record.oa_resp_total_tokens_sum,
      record.oa_chat_total_tokens_sum,
      record.claude_input_tokens_sum,
      record.claude_output_tokens_sum,
      record.gemini_total_token_count_sum
    ].filter((value) => Number.isFinite(value));
    if (!totals.length) {
      return 0;
    }
    const claudeTotal =
      (record.claude_input_tokens_sum || 0) + (record.claude_output_tokens_sum || 0);
    return Math.max(record.oa_resp_total_tokens_sum || 0,
      record.oa_chat_total_tokens_sum || 0,
      claudeTotal,
      record.gemini_total_token_count_sum || 0
    );
  };

  const usageModelLabel = (record) => {
    if (!record) {
      return "all";
    }
    const model = record.model || record.upstream_model;
    return model && String(model).trim() ? String(model) : "all";
  };

  const formatTimestamp = (unixTs) => {
    if (!unixTs || !Number.isFinite(unixTs)) {
      return "-";
    }
    const date = new Date(unixTs * 1000);
    return date.toLocaleString();
  };

  const formatDuration = (seconds) => {
    if (!seconds || !Number.isFinite(seconds)) {
      return "-";
    }
    const total = Math.max(0, Math.floor(seconds));
    const days = Math.floor(total / 86400);
    const hours = Math.floor((total % 86400) / 3600);
    const minutes = Math.floor((total % 3600) / 60);
    if (days > 0) {
      return `${days}d ${hours}h`;
    }
    if (hours > 0) {
      return `${hours}h ${minutes}m`;
    }
    return `${minutes}m`;
  };

  const formatLocalTime = (value) => {
    if (!value) {
      return "-";
    }
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) {
      return "-";
    }
    return date.toLocaleString();
  };

  const renderLiveUsage = (providerId, data) => {
    if (!data || typeof data !== "object") {
      return html`<div class="text-xs text-slate-500">No live usage data.</div>`;
    }
    if (providerId === "codex") {
      const rate = data.rate_limit || {};
      const primary = rate.primary_window || {};
      const secondary = rate.secondary_window || {};
      const review = data.code_review_rate_limit || {};
      const reviewPrimary = review.primary_window || {};
      const windowLabel = (window) => {
        const secs = window.limit_window_seconds || window.reset_after_seconds;
        if (secs === 18000) {
          return "5h window";
        }
        if (secs === 604800) {
          return "1w window";
        }
        return "Window";
      };
      return html`
        <div class="grid gap-2 md:grid-cols-2">
          <div class="rounded-lg border border-slate-200 bg-white px-3 py-2">
            <div class="text-xs font-semibold text-slate-600">${windowLabel(primary)}</div>
            <div class="mt-1 text-xs text-slate-500">Allowed: ${String(rate.allowed)}</div>
            <div class="text-xs text-slate-500">Limit reached: ${String(rate.limit_reached)}</div>
            <div class="text-xs text-slate-500">
              Reset at: ${formatTimestamp(primary.reset_at)}
            </div>
            <div class="text-xs text-slate-500">
              Used: ${primary.used_percent ?? "-"}%
            </div>
          </div>
          ${secondary && secondary.reset_at
            ? html`
                <div class="rounded-lg border border-slate-200 bg-white px-3 py-2">
                  <div class="text-xs font-semibold text-slate-600">${windowLabel(secondary)}</div>
                  <div class="mt-1 text-xs text-slate-500">
                    Reset at: ${formatTimestamp(secondary.reset_at)}
                  </div>
                  <div class="text-xs text-slate-500">
                    Reset in: ${formatDuration(secondary.reset_after_seconds)}
                  </div>
                  <div class="text-xs text-slate-500">
                    Used: ${secondary.used_percent ?? "-"}%
                  </div>
                </div>
              `
            : null}
        </div>
        ${reviewPrimary.reset_at
          ? html`
              <div class="mt-3 rounded-lg border border-indigo-200 bg-white px-3 py-2">
                <div class="text-xs font-semibold text-indigo-500">Code review</div>
                <div class="mt-1 text-xs text-slate-500">
                  ${windowLabel(reviewPrimary)}
                </div>
                <div class="text-xs text-slate-500">
                  Reset at: ${formatTimestamp(reviewPrimary.reset_at)}
                </div>
                <div class="text-xs text-slate-500">
                  Used: ${reviewPrimary.used_percent ?? "-"}%
                </div>
              </div>
            `
          : null}
      `;
    }
    if (providerId === "claudecode") {
      const windows = [
        { key: "five_hour", label: "5h window" },
        { key: "seven_day", label: "7d window" },
        { key: "seven_day_sonnet", label: "7d sonnet window" }
      ];
      return html`
        <div class="grid gap-2 md:grid-cols-2">
          ${windows.map((item) => {
            const block = data[item.key] || {};
            return html`
              <div class="rounded-lg border border-slate-200 bg-white px-3 py-2">
                <div class="text-xs font-semibold text-slate-600">${item.label}</div>
                <div class="mt-1 text-xs text-slate-500">
                  Utilization: ${block.utilization ?? "-"}%
                </div>
                <div class="text-xs text-slate-500">
                  Resets at: ${formatLocalTime(block.resets_at)}
                </div>
              </div>
            `;
          })}
        </div>
      `;
    }
    if (providerId === "antigravity") {
      const models = data.models || {};
      const entries = Object.entries(models);
      if (!entries.length) {
        return html`<div class="text-xs text-slate-500">No model usage available.</div>`;
      }
      const formatRemaining = (value) => {
        if (value === null || value === undefined || Number.isNaN(value)) {
          return "-";
        }
        const numeric = Number(value);
        if (numeric <= 1) {
          return `${Math.round(numeric * 100)}%`;
        }
        return `${Math.round(numeric)}%`;
      };
      return html`
        <div class="grid gap-2 md:grid-cols-2">
          ${entries.map(([model, info]) => html`
            <div class="rounded-lg border border-slate-200 bg-white px-3 py-2">
              <div class="text-xs font-semibold text-slate-600">${model}</div>
              <div class="mt-1 text-xs text-slate-500">
                Remaining: ${formatRemaining(info.remainingFraction ?? info.remaining_fraction)}
              </div>
              <div class="text-xs text-slate-500">
                Reset: ${formatLocalTime(info.resetTime ?? info.reset_time)}
              </div>
            </div>
          `)}
        </div>
      `;
    }
    return html`<pre class="text-xs text-slate-600">${JSON.stringify(data, null, 2)}</pre>`;
  };

  const fetchLiveUsage = async (providerId, adminKey) => {
    const headers = {};
    if (providerId === "codex") {
      headers.Authorization = `Bearer ${adminKey}`;
    } else if (providerId === "claudecode") {
      headers["x-api-key"] = adminKey;
      headers["anthropic-version"] = "2023-06-01";
    } else if (providerId === "antigravity") {
      headers["x-goog-api-key"] = adminKey;
    }
    const response = await fetch(`/${providerId}/usage`, { headers });
    if (!response.ok) {
      const text = await response.text();
      throw new Error(text || response.statusText);
    }
    return response.json();
  };

  const readFileAsText = (file) =>
    new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(reader.result);
      reader.onerror = () => reject(reader.error);
      reader.readAsText(file);
    });

  const safeJsonParse = (text) => {
    try {
      return { value: JSON.parse(text), error: null };
    } catch (error) {
      return { value: null, error };
    }
  };

  const apiRequest = async (path, options = {}) => {
    const headers = Object.assign(
      { Accept: "application/json" },
      options.headers || {}
    );
    if (options.adminKey) {
      headers["x-admin-key"] = options.adminKey;
    }
    const body = options.body ? JSON.stringify(options.body) : undefined;
    if (body) {
      headers["Content-Type"] = "application/json";
    }

    const response = await fetch(path, {
      method: options.method || "GET",
      headers,
      body
    });

    if (!response.ok) {
      const text = await response.text();
      const message = text || response.statusText;
      throw new Error(`${response.status} ${message}`);
    }

    if (response.status === 204) {
      return null;
    }

    const text = await response.text();
    if (!text) {
      return null;
    }

    return JSON.parse(text);
  };

  const parseCallbackInput = (input) => {
    if (!input) {
      return { error: "Callback input is empty." };
    }
    const trimmed = input.trim();
    let query = trimmed;
    if (trimmed.includes("http://") || trimmed.includes("https://")) {
      try {
        const url = new URL(trimmed);
        query = url.search.startsWith("?") ? url.search.slice(1) : url.search;
      } catch (_) {
        return { error: "Invalid URL in callback input." };
      }
    } else if (trimmed.includes("?")) {
      query = trimmed.split("?").slice(1).join("?");
    }
    const params = new URLSearchParams(query);
    const state = params.get("state");
    const code = params.get("code");
    if (!state || !code) {
      return { error: "state or code is missing in callback input." };
    }
    return { state, code };
  };

  const formatKeyValue = (value) => {
    if (value === null || value === undefined) {
      return "-";
    }
    const stringValue = String(value);
    if (stringValue.length <= 42) {
      return stringValue;
    }
    return `${stringValue.slice(0, 18)}...${stringValue.slice(-12)}`;
  };

  const getCredentialStatus = (states) => {
    if (!Array.isArray(states)) {
      return { state: "active" };
    }
    const now = Math.floor(Date.now() / 1000);
    const disabled = states.find((entry) => Array.isArray(entry) && entry[1] === "disabled");
    if (disabled) {
      return { state: "disabled" };
    }
    const cooldown = states.find(
      (entry) =>
        Array.isArray(entry) &&
        (entry[1] === "cooldown" || entry[1] === "cooldown_sonnet") &&
        entry[2] > now
    );
    if (cooldown) {
      return { state: "cooldown", until: cooldown[2] };
    }
    const transient = states.find(
      (entry) => Array.isArray(entry) && entry[1] === "transient" && entry[2] > now
    );
    if (transient) {
      return { state: "transient", until: transient[2] };
    }
    return { state: "active" };
  };

  const setDisabled = (credential, disabled) => {
    const next = Object.assign({}, credential);
    const currentStates = Array.isArray(next.states) ? next.states.slice() : [];
    const filtered = currentStates.filter(
      (entry) => !(Array.isArray(entry) && entry[1] === "disabled")
    );
    if (disabled) {
      filtered.push(["*", "disabled", 0]);
    }
    next.states = filtered;
    return next;
  };

  const credentialSummary = (provider, credential) => {
    const fields = provider.keyFields || [];
    const chunks = fields
      .map((field) => ({ field, value: credential[field] }))
      .filter((item) => item.value !== undefined && item.value !== "");
    if (chunks.length === 0) {
      return "Credential";
    }
    return chunks
      .map((item) => `${item.field}: ${formatKeyValue(item.value)}`)
      .join(" | ");
  };

  const TabBar = ({ tabs, active, onChange, size }) => {
    return html`
      <div class="tab-track rounded-2xl p-1 inline-flex gap-1 overflow-x-auto scrollbar-thin">
        ${tabs.map(
          (tab) => html`
            <button
              key=${tab.value}
              onClick=${() => onChange(tab.value)}
              class=${`px-4 py-2 rounded-xl text-sm font-medium transition-all ${
                active === tab.value
                  ? "bg-white text-slate-900 tab-pill-active"
                  : "text-slate-500 hover:text-slate-800 hover:bg-white/60"
              } ${size === "sm" ? "text-xs px-3 py-1.5" : ""}`}
            >
              ${tab.label}
            </button>
          `
        )}
      </div>
    `;
  };

  const StatusToast = ({ status }) => {
    if (!status) {
      return null;
    }
    const colorMap = {
      success: "bg-emerald-600",
      error: "bg-rose-600",
      info: "bg-sky-600",
      warning: "bg-amber-500"
    };
    return html`
      <div
        class=${`fixed top-6 right-6 z-50 px-4 py-3 rounded-xl text-white shadow-xl ${
          colorMap[status.type] || "bg-slate-700"
        }`}
      >
        <div class="text-sm font-medium">${status.message}</div>
      </div>
    `;
  };

  const UploadPanel = ({
    provider,
    adminKey,
    onStatus,
    refreshCredentials,
    showUpload = true,
    showOauth
  }) => {
    const [jsonText, setJsonText] = useState("");
    const [files, setFiles] = useState([]);
    const [busy, setBusy] = useState(false);
    const [oauthUrl, setOauthUrl] = useState("");
    const [oauthInput, setOauthInput] = useState("");
    const [oauthExtra, setOauthExtra] = useState("");

    const providerId = provider.id;
    const oauthParam = provider.oauthParam || "";
    const shouldShowOauth = showOauth === undefined ? provider.oauth : showOauth;
    const keyOnlyProvider = Array.isArray(provider.keyFields) && provider.keyFields.length === 1 && provider.keyFields[0] === "key";

    const readJsonPayloads = async () => {
      const payloads = [];
      if (jsonText.trim()) {
        if (keyOnlyProvider) {
          const lines = jsonText
            .split(/\r?\n/)
            .map((line) => line.trim())
            .filter((line) => line);
          lines.forEach((key) => payloads.push({ key }));
        } else {
          const parsed = safeJsonParse(jsonText.trim());
          if (parsed.error) {
            throw new Error("Invalid JSON in text input.");
          }
          if (Array.isArray(parsed.value)) {
            payloads.push(...parsed.value);
          } else {
            payloads.push(parsed.value);
          }
        }
      }

      for (const file of files) {
        const text = await readFileAsText(file);
        const parsed = safeJsonParse(text);
        if (parsed.error) {
          throw new Error(`Invalid JSON in file: ${file.name}`);
        }
        if (Array.isArray(parsed.value)) {
          payloads.push(...parsed.value);
        } else {
          payloads.push(parsed.value);
        }
      }

      return payloads;
    };

    const handleUpload = async () => {
      setBusy(true);
      try {
        const payloads = await readJsonPayloads();
        if (payloads.length === 0) {
          throw new Error("No JSON payloads to upload.");
        }
        for (const payload of payloads) {
          await apiRequest(`/admin/providers/${providerId}/credentials`, {
            method: "POST",
            body: payload,
            adminKey
          });
        }
        setJsonText("");
        setFiles([]);
        if (typeof refreshCredentials === "function") {
          refreshCredentials();
        }
        onStatus({ type: "success", message: `Uploaded ${payloads.length} credential(s).` });
        onStatus({ type: "success", message: `Uploaded ${payloads.length} credential(s).` });
      } catch (error) {
        onStatus({ type: "error", message: error.message });
      } finally {
        setBusy(false);
      }
    };

    const handleOauthStart = async () => {
      setBusy(true);
      try {
        const params = new URLSearchParams();
        if (oauthParam && oauthExtra.trim()) {
          params.set(oauthParam, oauthExtra.trim());
        }
        const qs = params.toString();
        const path = `/${providerId}/oauth${qs ? `?${qs}` : ""}`;
        const data = await apiRequest(path, {
          method: "GET",
          adminKey,
          headers: { Accept: "application/json" }
        });
        if (!data || !data.auth_url) {
          throw new Error("Failed to fetch OAuth URL.");
        }
        setOauthUrl(data.auth_url);
        onStatus({ type: "success", message: "OAuth URL created." });
      } catch (error) {
        onStatus({ type: "error", message: error.message });
      } finally {
        setBusy(false);
      }
    };

    const handleOauthCallback = async () => {
      setBusy(true);
      try {
        const parsed = parseCallbackInput(oauthInput);
        if (parsed.error) {
          throw new Error(parsed.error);
        }
        const params = new URLSearchParams();
        params.set("state", parsed.state);
        params.set("code", parsed.code);
        if (oauthParam && oauthExtra.trim()) {
          params.set(oauthParam, oauthExtra.trim());
        }
        await apiRequest(`/${providerId}/oauth/callback?${params.toString()}`, {
          method: "GET"
        });
        setOauthInput("");
        if (typeof refreshCredentials === "function") {
          refreshCredentials();
        }
        onStatus({ type: "success", message: "OAuth callback accepted." });
        onStatus({ type: "success", message: "OAuth callback accepted." });
      } catch (error) {
        onStatus({ type: "error", message: error.message });
      } finally {
        setBusy(false);
      }
    };

    return html`
      <div class="space-y-6">
        ${shouldShowOauth
          ? html`
              <div class="rounded-2xl border border-slate-200 bg-white/80 p-5 card-shadow">
                <div class="flex flex-col gap-3">
                  <div class="text-sm font-semibold text-slate-700">OAuth helper</div>
                  <div class="text-xs text-slate-500">
                    Copy the auth URL, finish sign-in, then paste the callback query string.
                  </div>
                  ${oauthParam
                    ? html`
                        <div>
                          <label class="text-xs uppercase tracking-wide text-slate-400">
                            ${oauthParam}
                          </label>
                          <input
                            value=${oauthExtra}
                            onInput=${(event) => setOauthExtra(event.target.value)}
                            placeholder=${`Optional ${oauthParam}`}
                            class="mt-1 w-full rounded-xl border border-slate-200 px-3 py-2 text-sm focus:border-indigo-400 focus:outline-none"
                          />
                        </div>
                      `
                    : null}
                  <div class="flex flex-wrap gap-3">
                    <button
                      class="px-4 py-2 rounded-xl bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-500 disabled:opacity-50"
                      onClick=${handleOauthStart}
                      disabled=${busy}
                    >
                      Get OAuth URL
                    </button>
                    ${oauthUrl
                      ? html`
                          <a
                            href=${oauthUrl}
                            target="_blank"
                            rel="noreferrer"
                            class="text-sm text-indigo-600 underline break-all"
                          >
                            ${oauthUrl}
                          </a>
                        `
                      : null}
                  </div>
                  <div>
                    <label class="text-xs uppercase tracking-wide text-slate-400">
                      Callback query string
                    </label>
                    <textarea
                      rows="3"
                      value=${oauthInput}
                      onInput=${(event) => setOauthInput(event.target.value)}
                      placeholder="state=...&code=..."
                      class="mt-2 w-full rounded-xl border border-slate-200 px-3 py-2 text-sm focus:border-indigo-400 focus:outline-none"
                    ></textarea>
                  </div>
                  <button
                    class="px-4 py-2 rounded-xl bg-slate-900 text-white text-sm font-medium hover:bg-slate-800 disabled:opacity-50"
                    onClick=${handleOauthCallback}
                    disabled=${busy}
                  >
                    Submit OAuth callback
                  </button>
                </div>
              </div>
            `
          : null}
        ${showUpload
          ? html`
              <div class="rounded-2xl border border-slate-200 bg-white/80 p-5 card-shadow">
          <div class="flex flex-col gap-4">
            <div class="text-sm font-semibold text-slate-700">Upload credentials</div>
            <div>
              <label class="text-xs uppercase tracking-wide text-slate-400">
                ${keyOnlyProvider ? "Keys (one per line)" : "JSON input"}
              </label>
              <textarea
                rows="5"
                value=${jsonText}
                onInput=${(event) => setJsonText(event.target.value)}
                placeholder=${keyOnlyProvider
                  ? "sk-... (one key per line)"
                  : "Paste credential JSON or array"}
                class="mt-2 w-full rounded-xl border border-slate-200 px-3 py-2 text-sm focus:border-indigo-400 focus:outline-none"
              ></textarea>
            </div>
            <div>
              <label class="text-xs uppercase tracking-wide text-slate-400">Files</label>
              <div class="mt-2 flex flex-wrap items-center gap-3">
                <label class="px-4 py-2 rounded-xl bg-slate-900 text-white text-sm font-medium hover:bg-slate-800 cursor-pointer">
                  Select files
                  <input
                    type="file"
                    multiple
                    onChange=${(event) => setFiles(Array.from(event.target.files || []))}
                    class="hidden"
                  />
                </label>
                ${files.length
                  ? html`<div class="text-xs text-slate-500">${files.length} file(s) selected.</div>`
                  : html`<div class="text-xs text-slate-500">No files selected.</div>`}
              </div>
            </div>
            <button
              class="px-4 py-2 rounded-xl bg-emerald-600 text-white text-sm font-medium hover:bg-emerald-500 disabled:opacity-50"
              onClick=${handleUpload}
              disabled=${busy}
            >
              Upload credentials
            </button>
          </div>
        </div>
            `
          : null}
      </div>
    `;
  };

  const CredentialsPanel = ({ provider, adminKey, onStatus }) => {
    const [credentials, setCredentials] = useState([]);
    const [loading, setLoading] = useState(false);
    const [expanded, setExpanded] = useState({});
    const [usageExpanded, setUsageExpanded] = useState({});
    const [usageLoading, setUsageLoading] = useState({});
    const [usageData, setUsageData] = useState({});
    const [liveUsage, setLiveUsage] = useState({});

    const loadCredentials = async () => {
      setLoading(true);
      try {
        const data = await apiRequest(`/admin/providers/${provider.id}/credentials`, {
          adminKey
        });
        setCredentials(Array.isArray(data) ? data : []);
      } catch (error) {
        onStatus({ type: "error", message: error.message });
      } finally {
        setLoading(false);
      }
    };

    useEffect(() => {
      loadCredentials();
    }, [provider.id]);

    const updateCredential = async (index, next) => {
      try {
        await apiRequest(`/admin/providers/${provider.id}/credentials/${index}`, {
          method: "PUT",
          body: next,
          adminKey
        });
        await loadCredentials();
        onStatus({ type: "success", message: "Credential updated." });
      } catch (error) {
        onStatus({ type: "error", message: error.message });
      }
    };

    const deleteCredential = async (index) => {
      if (!confirm("Delete this credential?")) {
        return;
      }
      try {
        await apiRequest(`/admin/providers/${provider.id}/credentials/${index}`, {
          method: "DELETE",
          adminKey
        });
        await loadCredentials();
        onStatus({ type: "success", message: "Credential deleted." });
      } catch (error) {
        onStatus({ type: "error", message: error.message });
      }
    };

    const toggleDisabled = async (index, credential) => {
      const status = getCredentialStatus(credential.states);
      const next = setDisabled(credential, status.state !== "disabled");
      await updateCredential(index, next);
    };

    const toggleUsage = async (index, credential) => {
      const nextExpanded = !usageExpanded[index];
      setUsageExpanded((prev) => ({ ...prev, [index]: nextExpanded }));
      if (!nextExpanded || usageLoading[index]) {
        return;
      }
      const credentialId = usageCredentialId(provider.id, credential);
      if (!credentialId) {
        onStatus({ type: "warning", message: "Usage credential id missing." });
        return;
      }
      setUsageLoading((prev) => ({ ...prev, [index]: true }));
      try {
        const data = await apiRequest(
          `/admin/usage/provider/${provider.id}?credential_id=${encodeURIComponent(credentialId)}`,
          { adminKey }
        );
        setUsageData((prev) => ({ ...prev, [index]: Array.isArray(data) ? data : [] }));
        if (SPECIAL_USAGE_PROVIDERS.has(provider.id)) {
          const live = await fetchLiveUsage(provider.id, adminKey);
          setLiveUsage((prev) => ({ ...prev, [index]: live }));
        }
      } catch (error) {
        onStatus({ type: "error", message: error.message });
      } finally {
        setUsageLoading((prev) => ({ ...prev, [index]: false }));
      }
    };

    const groupedUsage = (records) => {
      const grouped = {};
      (records || []).forEach((record) => {
        const key = usageModelLabel(record);
        if (!grouped[key]) {
          grouped[key] = [];
        }
        grouped[key].push(record);
      });
      return grouped;
    };

    return html`
      <div class="rounded-2xl border border-slate-200 bg-white/80 p-5 card-shadow">
        <div class="flex items-center justify-between">
          <div class="text-sm font-semibold text-slate-700">Credential list</div>
          <button
            class="text-xs uppercase tracking-wide text-indigo-600"
            onClick=${loadCredentials}
          >
            Refresh
          </button>
        </div>
        ${loading
          ? html`<div class="py-6 text-sm text-slate-500">Loading...</div>`
          : null}
        ${!loading && credentials.length === 0
          ? html`<div class="py-6 text-sm text-slate-500">No credentials uploaded.</div>`
          : null}
        <div class="mt-4 space-y-4">
          ${credentials.map((credential, index) => {
            const status = getCredentialStatus(credential.states);
            return html`
              <div key=${index} class="rounded-2xl border border-slate-200 bg-white p-4">
                <div class="flex flex-wrap items-start justify-between gap-4">
                  <div>
                    <div class="text-sm font-semibold text-slate-800">
                      ${credentialSummary(provider, credential)}
                    </div>
                    <div class="mt-1 text-xs text-slate-400">Index ${index}</div>
                  </div>
                  <div class="flex flex-wrap items-center gap-2">
                    <span
                      class=${`text-xs px-3 py-1 rounded-full border ${
                        STATUS_COLORS[status.state]
                      }`}
                    >
                      ${status.state}
                    </span>
                    <button
                      class="text-xs px-3 py-1 rounded-full border border-slate-200 hover:bg-slate-100"
                      onClick=${() => toggleUsage(index, credential)}
                    >
                      ${usageExpanded[index] ? "Hide usage" : "Usage"}
                    </button>
                    <button
                      class="text-xs px-3 py-1 rounded-full border border-slate-200 hover:bg-slate-100"
                      onClick=${() => toggleDisabled(index, credential)}
                    >
                      ${status.state === "disabled" ? "Enable" : "Disable"}
                    </button>
                    <button
                      class="text-xs px-3 py-1 rounded-full border border-slate-200 hover:bg-slate-100"
                      onClick=${() =>
                        setExpanded((prev) => ({
                          ...prev,
                          [index]: !prev[index]
                        }))}
                    >
                      ${expanded[index] ? "Hide" : "View"}
                    </button>
                    <button
                      class="text-xs px-3 py-1 rounded-full border border-rose-200 text-rose-600 hover:bg-rose-50"
                      onClick=${() => deleteCredential(index)}
                    >
                      Delete
                    </button>
                  </div>
                </div>
                ${expanded[index]
                  ? html`
                      <pre class="mt-3 whitespace-pre-wrap text-xs bg-slate-50 border border-slate-200 rounded-xl p-3 text-slate-700 min-w-[900px] overflow-x-auto">
${JSON.stringify(credential, null, 2)}
                      </pre>
                    `
                  : null}
                ${usageExpanded[index]
                  ? html`
                      <div class="mt-3 rounded-xl border border-slate-200 bg-slate-50 p-3">
                        <div class="text-xs uppercase tracking-wide text-slate-400">
                          Usage views
                        </div>
                        ${usageLoading[index]
                          ? html`<div class="mt-2 text-xs text-slate-500">Loading usage...</div>`
                          : null}
                        ${!usageLoading[index] &&
                        (!usageData[index] || usageData[index].length === 0)
                          ? html`<div class="mt-2 text-xs text-slate-500">No usage data.</div>`
                          : null}
                        ${!usageLoading[index] && usageData[index]
                          ? html`
                              <div class="mt-3 grid gap-3">
                                ${Object.entries(groupedUsage(usageData[index])).map(
                                  ([model, records]) => html`
                                    <div
                                      key=${model}
                                      class="rounded-lg border border-slate-200 bg-white px-3 py-2"
                                    >
                                      <div class="text-xs font-semibold text-slate-600">
                                        Model: ${model}
                                      </div>
                                      <div class="mt-2 grid gap-2 md:grid-cols-2">
                                        ${records.map((record) => {
                                          const total = usageTotalTokens(record);
                                          return html`
                                            <div
                                              key=${`${record.view_name}-${record.slot_start}`}
                                              class="rounded-lg border border-slate-200 bg-slate-50 px-3 py-2"
                                            >
                                              <div class="text-xs font-semibold text-slate-600">
                                                ${record.view_name}
                                              </div>
                                              <div class="mt-1 text-xs text-slate-500">
                                                Records: ${record.record_count}
                                              </div>
                                              <div class="mt-1 text-xs text-slate-500">
                                                Tokens: ${total}
                                              </div>
                                            </div>
                                          `;
                                        })}
                                      </div>
                                    </div>
                                  `
                                )}
                              </div>
                            `
                          : null}
                        ${SPECIAL_USAGE_PROVIDERS.has(provider.id)
                          ? html`
                              <div class="mt-4 rounded-lg border border-indigo-200 bg-white px-3 py-2">
                                <div class="text-xs uppercase tracking-wide text-indigo-400">
                                  Live usage
                                </div>
                                <div class="mt-2">
                                  ${renderLiveUsage(provider.id, liveUsage[index])}
                                </div>
                              </div>
                            `
                          : null}
                      </div>
                    `
                  : null}
              </div>
            `;
          })}
        </div>
      </div>
    `;
  };

  const ChannelPage = ({ adminKey, onStatus }) => {
    const [activeChannel, setActiveChannel] = useState(CHANNELS[0].id);
    const [subTab, setSubTab] = useState("upload");

    const provider = useMemo(
      () => CHANNELS.find((item) => item.id === activeChannel),
      [activeChannel]
    );

    return html`
      <div class="space-y-6">
        ${TabBar({
          tabs: CHANNELS.map((item) => ({ value: item.id, label: item.label })),
          active: activeChannel,
          onChange: setActiveChannel,
          size: "sm"
        })}
        <div class="flex gap-2">
          ${TabBar({
            tabs: [
              { value: "upload", label: "Upload" },
              { value: "manage", label: "Manage" }
            ],
            active: subTab,
            onChange: setSubTab,
            size: "sm"
          })}
        </div>
        ${subTab === "upload"
          ? html`
              <${UploadPanel}
                provider=${provider}
                adminKey=${adminKey}
                onStatus=${onStatus}
                refreshCredentials=${() => null}
              />
            `
          : html`
              <${CredentialsPanel}
                provider=${provider}
                adminKey=${adminKey}
                onStatus=${onStatus}
              />
            `}
      </div>
    `;
  };

  const DedicatedProviderPage = ({ providerId, adminKey, onStatus }) => {
    const provider = useMemo(
      () => PROVIDERS.find((item) => item.id === providerId),
      [providerId]
    );
    const [subTab, setSubTab] = useState("oauth");

    return html`
      <div class="space-y-6">
        ${TabBar({
          tabs: [
            { value: "oauth", label: "OAuth" },
            { value: "manage", label: "Credentials" }
          ],
          active: subTab,
          onChange: setSubTab,
          size: "sm"
        })}
        ${subTab === "oauth"
          ? html`
              <${UploadPanel}
                provider=${provider}
                adminKey=${adminKey}
                onStatus=${onStatus}
                refreshCredentials=${() => null}
                showUpload=${false}
                showOauth=${true}
              />
            `
          : html`
              <${CredentialsPanel}
                provider=${provider}
                adminKey=${adminKey}
                onStatus=${onStatus}
              />
            `}
      </div>
    `;
  };

  const BatchUploadPage = ({ adminKey, onStatus }) => {
    const [selected, setSelected] = useState(BATCH_PROVIDERS[0]);
    const [jsonText, setJsonText] = useState("");
    const [files, setFiles] = useState([]);
    const [busy, setBusy] = useState(false);

    const uploadBatch = async () => {
      setBusy(true);
      try {
        const payloads = [];
        if (jsonText.trim()) {
          const parsed = safeJsonParse(jsonText.trim());
          if (parsed.error) {
            throw new Error("Invalid JSON in text input.");
          }
          if (Array.isArray(parsed.value)) {
            payloads.push(...parsed.value);
          } else {
            payloads.push(parsed.value);
          }
        }
        for (const file of files) {
          const text = await readFileAsText(file);
          const parsed = safeJsonParse(text);
          if (parsed.error) {
            throw new Error(`Invalid JSON in file: ${file.name}`);
          }
          if (Array.isArray(parsed.value)) {
            payloads.push(...parsed.value);
          } else {
            payloads.push(parsed.value);
          }
        }
        if (payloads.length === 0) {
          throw new Error("No JSON payloads to upload.");
        }
        for (const payload of payloads) {
          await apiRequest(`/admin/providers/${selected}/credentials`, {
            method: "POST",
            body: payload,
            adminKey
          });
        }
        setJsonText("");
        setFiles([]);
        onStatus({ type: "success", message: `Uploaded ${payloads.length} credential(s).` });
      } catch (error) {
        onStatus({ type: "error", message: error.message });
      } finally {
        setBusy(false);
      }
    };

    return html`
      <div class="space-y-6">
        <div class="rounded-2xl border border-slate-200 bg-white/80 p-5 card-shadow">
          <div class="text-sm font-semibold text-slate-700">Batch upload</div>
          <div class="mt-4 grid gap-4">
            <div>
              <label class="text-xs uppercase tracking-wide text-slate-400">Provider</label>
              <select
                value=${selected}
                onChange=${(event) => setSelected(event.target.value)}
                class="mt-2 w-full rounded-xl border border-slate-200 px-3 py-2 text-sm"
              >
                ${BATCH_PROVIDERS.map(
                  (providerId) =>
                    html`<option value=${providerId}>${providerId}</option>`
                )}
              </select>
            </div>
            <div>
              <label class="text-xs uppercase tracking-wide text-slate-400">JSON input</label>
              <textarea
                rows="6"
                value=${jsonText}
                onInput=${(event) => setJsonText(event.target.value)}
                placeholder="Paste array or JSON payload"
                class="mt-2 w-full rounded-xl border border-slate-200 px-3 py-2 text-sm"
              ></textarea>
            </div>
            <div>
              <label class="text-xs uppercase tracking-wide text-slate-400">Files</label>
              <div class="mt-2 flex flex-wrap items-center gap-3">
                <label class="px-4 py-2 rounded-xl bg-slate-900 text-white text-sm font-medium hover:bg-slate-800 cursor-pointer">
                  Select files
                  <input
                    type="file"
                    multiple
                    onChange=${(event) => setFiles(Array.from(event.target.files || []))}
                    class="hidden"
                  />
                </label>
                ${files.length
                  ? html`<div class="text-xs text-slate-500">${files.length} file(s) selected.</div>`
                  : html`<div class="text-xs text-slate-500">No files selected.</div>`}
              </div>
            </div>
            <button
              class="px-4 py-2 rounded-xl bg-emerald-600 text-white text-sm font-medium hover:bg-emerald-500 disabled:opacity-50"
              onClick=${uploadBatch}
              disabled=${busy}
            >
              Upload batch
            </button>
          </div>
        </div>
      </div>
    `;
  };

  const ConfigPage = ({ adminKey, onStatus }) => {
    const [configs, setConfigs] = useState({});
    const [loading, setLoading] = useState(false);

    const loadConfigs = async () => {
      setLoading(true);
      try {
        const next = {};
        for (const provider of PROVIDERS) {
          try {
            const data = await apiRequest(`/admin/providers/${provider.id}/config`, {
              adminKey
            });
            next[provider.id] = {
              data,
              draft: data ? Object.assign({}, data) : null
            };
          } catch (_) {
            next[provider.id] = { data: null, draft: null };
          }
        }
        setConfigs(next);
      } catch (error) {
        onStatus({ type: "error", message: error.message });
      } finally {
        setLoading(false);
      }
    };

    useEffect(() => {
      loadConfigs();
    }, []);

    const updateBaseUrl = (providerId, value) => {
      setConfigs((prev) => {
        const next = Object.assign({}, prev);
        const entry = Object.assign({}, next[providerId]);
        const draft = Object.assign({}, entry.draft || {});
        draft.base_url = value;
        entry.draft = draft;
        next[providerId] = entry;
        return next;
      });
    };

    const saveConfig = async (providerId) => {
      const entry = configs[providerId];
      if (!entry || !entry.draft) {
        return;
      }
      try {
        await apiRequest(`/admin/providers/${providerId}/config`, {
          method: "PUT",
          body: entry.draft,
          adminKey
        });
        onStatus({ type: "success", message: `Saved ${providerId} config.` });
      } catch (error) {
        onStatus({ type: "error", message: error.message });
      }
    };

    return html`
      <div class="space-y-6">
        <div class="rounded-2xl border border-slate-200 bg-white/80 p-5 card-shadow">
          <div class="flex items-center justify-between">
            <div class="text-sm font-semibold text-slate-700">Provider settings</div>
            <button
              class="text-xs uppercase tracking-wide text-indigo-600"
              onClick=${loadConfigs}
            >
              Refresh
            </button>
          </div>
          <div class="mt-2 text-xs text-slate-500">
            Only base_url is editable. Other proxy settings are not exposed here.
          </div>
          ${loading
            ? html`<div class="py-6 text-sm text-slate-500">Loading...</div>`
            : html`
                <div class="mt-4 grid gap-4">
                  ${PROVIDERS.map((provider) => {
                    const entry = configs[provider.id] || {};
                    const draft = entry.draft || {};
                    if (!draft.base_url) {
                      return html`
                        <div key=${provider.id} class="border border-slate-200 rounded-xl p-4">
                          <div class="text-sm font-semibold text-slate-700">${provider.label}</div>
                          <div class="text-xs text-slate-400 mt-2">No config available.</div>
                        </div>
                      `;
                    }
                    return html`
                      <div key=${provider.id} class="border border-slate-200 rounded-xl p-4">
                        <div class="flex flex-wrap items-center justify-between gap-3">
                          <div>
                            <div class="text-sm font-semibold text-slate-700">${provider.label}</div>
                            <div class="text-xs text-slate-400">${provider.id}</div>
                          </div>
                          <button
                            class="text-xs px-3 py-1 rounded-full border border-slate-200 hover:bg-slate-100"
                            onClick=${() => saveConfig(provider.id)}
                          >
                            Save
                          </button>
                        </div>
                        <div class="mt-3">
                          <label class="text-xs uppercase tracking-wide text-slate-400">
                            base_url
                          </label>
                          <input
                            value=${draft.base_url}
                            onInput=${(event) =>
                              updateBaseUrl(provider.id, event.target.value)}
                            class="mt-2 w-full rounded-xl border border-slate-200 px-3 py-2 text-sm"
                          />
                        </div>
                        ${draft.rotate_num !== undefined
                          ? html`
                              <div class="mt-3 text-xs text-slate-500">
                                rotate_num: ${draft.rotate_num}
                              </div>
                            `
                          : null}
                      </div>
                    `;
                  })}
                </div>
              `}
        </div>
      </div>
    `;
  };

  const AboutPage = () => {
    return html`
      <div class="rounded-2xl border border-slate-200 bg-white/80 p-5 card-shadow">
        <div class="text-sm font-semibold text-slate-700">About this panel</div>
        <div class="mt-3 text-sm text-slate-600 leading-relaxed">
          This control panel manages credentials for all gproxy channels. Use the channel view to
          upload or manage credentials, and dedicated pages for Gemini CLI and Antigravity OAuth
          flows. OAuth callbacks require manual copy of the state and code query string.
        </div>
        <div class="mt-4 text-xs text-slate-500">
          API base: /admin for management, /{provider}/oauth for OAuth start.
        </div>
      </div>
    `;
  };

  const App = () => {
    const [adminKey, setAdminKey] = useState(localStorage.getItem("gproxy_admin_key") || "");
    const [authed, setAuthed] = useState(false);
    const [loginKey, setLoginKey] = useState("");
    const [status, setStatus] = useState(null);
    const [activeTab, setActiveTab] = useState(DEFAULT_TAB);

    useEffect(() => {
      if (!status) {
        return;
      }
      const timer = setTimeout(() => setStatus(null), 3200);
      return () => clearTimeout(timer);
    }, [status]);

    const validateKey = async (key) => {
      try {
        await apiRequest("/admin/config", { adminKey: key });
        setAdminKey(key);
        localStorage.setItem("gproxy_admin_key", key);
        setAuthed(true);
        setStatus({ type: "success", message: "Authenticated." });
      } catch (error) {
        setStatus({ type: "error", message: "Authentication failed." });
      }
    };

    useEffect(() => {
      if (adminKey) {
        validateKey(adminKey);
      }
    }, []);

    const logout = () => {
      localStorage.removeItem("gproxy_admin_key");
      setAdminKey("");
      setAuthed(false);
      setLoginKey("");
    };

    if (!authed) {
      return html`
        <div class="min-h-screen flex items-center justify-center px-6">
          <div class="w-full max-w-md rounded-3xl bg-white/90 border border-slate-200 p-8 card-shadow">
            <div class="text-xl font-semibold text-slate-800">gproxy control panel</div>
            <div class="mt-2 text-sm text-slate-500">Enter admin key to continue.</div>
            <input
              type="password"
              value=${loginKey}
              onInput=${(event) => setLoginKey(event.target.value)}
              class="mt-6 w-full rounded-xl border border-slate-200 px-4 py-3 text-sm focus:border-indigo-400 focus:outline-none"
              placeholder="Admin key"
            />
            <button
              class="mt-4 w-full rounded-xl bg-indigo-600 text-white py-3 text-sm font-semibold hover:bg-indigo-500"
              onClick=${() => validateKey(loginKey)}
            >
              Sign in
            </button>
          </div>
          <${StatusToast} status=${status} />
        </div>
      `;
    }

    return html`
      <div class="min-h-screen px-6 py-10">
        <${StatusToast} status=${status} />
        <div class="mx-auto max-w-6xl">
          <div class="flex flex-wrap items-center justify-between gap-4">
            <div>
              <div class="text-2xl font-semibold text-slate-900">gproxy control panel</div>
              <div class="text-xs text-slate-500">React + Tailwind front for credential ops.</div>
            </div>
            <button
              class="px-4 py-2 rounded-xl bg-slate-900 text-white text-sm"
              onClick=${logout}
            >
              Sign out
            </button>
          </div>

          <div class="mt-6">
            ${TabBar({
              tabs: [
                { value: "channels", label: "Channels" },
                { value: "batch", label: "Batch Upload" },
                { value: "config", label: "Config" },
                { value: "about", label: "About" }
              ],
              active: activeTab,
              onChange: setActiveTab
            })}
          </div>

          <div class="mt-8">
            ${activeTab === "channels"
              ? html`<${ChannelPage} adminKey=${adminKey} onStatus=${setStatus} />`
              : null}
            ${activeTab === "geminicli"
              ? html`<${DedicatedProviderPage}
                  providerId="geminicli"
                  adminKey=${adminKey}
                  onStatus=${setStatus}
                />`
              : null}
            ${activeTab === "antigravity"
              ? html`<${DedicatedProviderPage}
                  providerId="antigravity"
                  adminKey=${adminKey}
                  onStatus=${setStatus}
                />`
              : null}
            ${activeTab === "batch"
              ? html`<${BatchUploadPage} adminKey=${adminKey} onStatus=${setStatus} />`
              : null}
            ${activeTab === "config"
              ? html`<${ConfigPage} adminKey=${adminKey} onStatus=${setStatus} />`
              : null}
            ${activeTab === "about" ? html`<${AboutPage} />` : null}
          </div>
        </div>
      </div>
    `;
  };

  ReactDOM.createRoot(document.getElementById("root")).render(html`<${App} />`);
})();

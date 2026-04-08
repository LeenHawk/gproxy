export type LiveUsageRow = {
  name: string;
  percent: number | null;
  resetAt: string | number | null;
};

function asRecord(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return null;
  }
  return value as Record<string, unknown>;
}

function asNumber(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }
  if (typeof value === "string") {
    const trimmed = value.trim();
    if (!trimmed) {
      return null;
    }
    const parsed = Number(trimmed);
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
}

function toUsagePercent(value: unknown, mode: "used_percent" | "remaining_fraction"): number | null {
  const raw = asNumber(value);
  if (raw === null) {
    return null;
  }
  if (mode === "remaining_fraction") {
    return raw <= 1 ? (1 - raw) * 100 : 100 - raw;
  }
  return raw;
}

function toResetAt(value: unknown): string | number | null {
  if (typeof value === "string" && value.trim()) {
    return value.trim();
  }
  const raw = asNumber(value);
  if (raw === null) {
    return null;
  }
  return raw < 1_000_000_000_000 ? raw * 1000 : raw;
}

function pushLiveRow(
  rows: LiveUsageRow[],
  name: string,
  percent: number | null,
  resetAt: string | number | null,
) {
  if (percent === null && resetAt === null) {
    return;
  }
  rows.push({ name, percent, resetAt });
}

function normalizeModelLabel(value: string): string {
  const trimmed = value.trim().replace(/^\/+/, "");
  if (!trimmed) {
    return "";
  }
  if (trimmed.startsWith("models/")) {
    return trimmed.slice("models/".length).trim();
  }
  return trimmed;
}

function parseCodexUsage(payload: Record<string, unknown>): LiveUsageRow[] {
  const rows: LiveUsageRow[] = [];
  const rateLimit = asRecord(payload.rate_limit);
  if (rateLimit) {
    const primary = asRecord(rateLimit.primary_window);
    const secondary = asRecord(rateLimit.secondary_window);
    if (primary) {
      pushLiveRow(
        rows,
        "primary",
        toUsagePercent(primary.used_percent, "used_percent"),
        toResetAt(primary.reset_at ?? primary.resetAt),
      );
    }
    if (secondary) {
      pushLiveRow(
        rows,
        "secondary",
        toUsagePercent(secondary.used_percent, "used_percent"),
        toResetAt(secondary.reset_at ?? secondary.resetAt),
      );
    }
  }
  return rows;
}

function parseClaudeCodeUsage(payload: Record<string, unknown>): LiveUsageRow[] {
  const rows: LiveUsageRow[] = [];
  for (const [name, value] of Object.entries(payload)) {
    const section = asRecord(value);
    if (!section) {
      continue;
    }
    pushLiveRow(
      rows,
      name,
      toUsagePercent(section.utilization, "used_percent"),
      toResetAt(section.resets_at ?? section.resetAt),
    );
  }
  return rows;
}

function parseGeminiCliUsage(payload: Record<string, unknown>): LiveUsageRow[] {
  const rows: LiveUsageRow[] = [];
  const buckets = Array.isArray(payload.buckets) ? payload.buckets : [];
  for (const item of buckets) {
    const bucket = asRecord(item);
    if (!bucket) {
      continue;
    }
    const modelIdRaw = typeof bucket.modelId === "string" ? bucket.modelId : "unknown";
    const modelId = normalizeModelLabel(modelIdRaw) || "unknown";
    const tokenType = typeof bucket.tokenType === "string" ? bucket.tokenType : "";
    const name = tokenType ? `${modelId} (${tokenType})` : modelId;
    pushLiveRow(
      rows,
      name,
      toUsagePercent(bucket.remainingFraction, "remaining_fraction"),
      toResetAt(bucket.resetTime),
    );
  }
  return rows;
}

function parseAntigravityUsage(payload: Record<string, unknown>): LiveUsageRow[] {
  const rows: LiveUsageRow[] = [];
  const models = asRecord(payload.models);
  if (!models) {
    return rows;
  }
  for (const [modelId, raw] of Object.entries(models)) {
    const model = asRecord(raw);
    if (!model) {
      continue;
    }
    const quota = asRecord(model.quotaInfo);
    pushLiveRow(
      rows,
      normalizeModelLabel(modelId) || modelId,
      toUsagePercent(quota?.remainingFraction, "remaining_fraction"),
      toResetAt(quota?.resetTime),
    );
  }
  return rows;
}

export function supportsCredentialUsageChannel(channel: string): boolean {
  return ["codex", "claudecode", "geminicli", "antigravity"].includes(
    channel.trim().toLowerCase(),
  );
}

export function parseLiveUsageRows(channel: string, payload: unknown): LiveUsageRow[] {
  const root = asRecord(payload);
  if (!root) {
    return [];
  }
  switch (channel.trim().toLowerCase()) {
    case "codex":
      return parseCodexUsage(root);
    case "claudecode":
      return parseClaudeCodeUsage(root);
    case "geminicli":
      return parseGeminiCliUsage(root);
    case "antigravity":
      return parseAntigravityUsage(root);
    default:
      return [];
  }
}

export function formatUsagePercent(value: number | null): string {
  if (value === null || Number.isNaN(value)) {
    return "—";
  }
  return `${value.toFixed(value % 1 === 0 ? 0 : 1)}%`;
}

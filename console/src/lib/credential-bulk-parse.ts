import type { SecretFamily } from "@/lib/channel-meta";

/** Input mode per credential family: bare tokens line-by-line, or JSON objects. */
export type BulkMode = "tokens" | "json";

export function bulkModeFor(family: SecretFamily): BulkMode {
  return family === "api_key" || family === "github_token" ? "tokens" : "json";
}

export interface BulkItem {
  label: string | null;
  /** PLAINTEXT secret_json for one credential. */
  secret: unknown;
  /** Short identifying text for result display (token prefix / email / label). */
  display: string;
}

export interface BulkError {
  /** Locale-neutral source ref: "L3" for textarea lines, "a.json#2" for files. */
  source: string;
  code: "invalid_json" | "invalid_secret";
}

export interface ParseOutcome {
  items: BulkItem[];
  errors: BulkError[];
}

const TOKEN_FIELD: Partial<Record<SecretFamily, string>> = {
  api_key: "api_key",
  github_token: "github_token",
};

/** Token mode: one bare key per line, optional `,label` / `<TAB>label`, `#` comments. */
export function parseTokens(family: SecretFamily, text: string): ParseOutcome {
  const field = TOKEN_FIELD[family] ?? "api_key";
  const items: BulkItem[] = [];
  for (const raw of text.split("\n")) {
    const t = raw.trim();
    if (!t || t.startsWith("#")) continue;
    const sep = t.search(/[,\t]/);
    const key = sep === -1 ? t : t.slice(0, sep).trim();
    const label = sep === -1 ? null : t.slice(sep + 1).trim() || null;
    if (!key) continue;
    items.push({ label, secret: { [field]: key }, display: prefix(key) });
  }
  return { items, errors: [] };
}

/**
 * JSON mode: auto-detects a JSON array, a single object (e.g. a pretty-printed
 * service-account file), or JSONL (one object per line). Each object is the
 * secret itself, or a `{"label": ..., "secret": {...}}` wrapper.
 */
export function parseJsonInput(family: SecretFamily, text: string, source: string): ParseOutcome {
  const trimmed = text.trim();
  if (trimmed === "") return { items: [], errors: [] };

  if (trimmed.startsWith("[")) {
    const parsed = tryJson(trimmed);
    if (parsed === undefined || !Array.isArray(parsed)) {
      return { items: [], errors: [{ source, code: "invalid_json" }] };
    }
    return collect(family, parsed.map((value, i) => ({ value, source: `${source}#${i + 1}` })));
  }

  // Whole-text object (multi-line pretty JSON) before falling back to JSONL.
  const whole = tryJson(trimmed);
  if (whole !== undefined) return collect(family, [{ value: whole, source }]);

  const entries: { value: unknown; source: string }[] = [];
  const errors: BulkError[] = [];
  trimmed.split("\n").forEach((raw, i) => {
    const line = raw.trim();
    if (!line || line.startsWith("#")) return;
    const lineSource = `${source}#L${i + 1}`;
    const value = tryJson(line);
    if (value === undefined) errors.push({ source: lineSource, code: "invalid_json" });
    else entries.push({ value, source: lineSource });
  });
  const outcome = collect(family, entries);
  return { items: outcome.items, errors: [...errors, ...outcome.errors] };
}

/** Drop items with identical secrets (per JSON text); returns the removed count. */
export function dedupeItems(items: BulkItem[]): { items: BulkItem[]; dupes: number } {
  const seen = new Set<string>();
  const kept: BulkItem[] = [];
  for (const item of items) {
    const key = JSON.stringify(item.secret);
    if (seen.has(key)) continue;
    seen.add(key);
    kept.push(item);
  }
  return { items: kept, dupes: items.length - kept.length };
}

function collect(family: SecretFamily, entries: { value: unknown; source: string }[]): ParseOutcome {
  const items: BulkItem[] = [];
  const errors: BulkError[] = [];
  for (const { value, source } of entries) {
    const item = toItem(family, value, source);
    if (item) items.push(item);
    else errors.push({ source, code: "invalid_secret" });
  }
  return { items, errors };
}

function toItem(family: SecretFamily, value: unknown, source: string): BulkItem | null {
  if (!isObject(value)) return null;
  let label: string | null = null;
  let secret: Record<string, unknown> = value;
  // Optional wrapper: { label, secret: {...} }.
  if (isObject(value.secret)) {
    secret = value.secret;
    label = typeof value.label === "string" && value.label.trim() !== "" ? value.label : null;
  }
  if (Object.keys(secret).length === 0) return null;
  if (family === "service_account") {
    if (!nonEmptyString(secret.client_email) || !nonEmptyString(secret.private_key)) return null;
  }
  return { label, secret, display: displayOf(label, secret, source) };
}

function displayOf(label: string | null, secret: Record<string, unknown>, source: string): string {
  if (label) return label;
  if (nonEmptyString(secret.client_email)) return secret.client_email;
  for (const field of ["refresh_token", "access_token", "cookie"]) {
    const v = secret[field];
    if (nonEmptyString(v)) return prefix(v);
  }
  return source;
}

function prefix(token: string): string {
  return token.slice(0, 8) + "…";
}

function tryJson(text: string): unknown {
  try {
    return JSON.parse(text) as unknown;
  } catch {
    return undefined;
  }
}

function isObject(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

function nonEmptyString(v: unknown): v is string {
  return typeof v === "string" && v.trim() !== "";
}

import { clearSession } from "../app/session";

export class ApiError extends Error {
  status: number;

  constructor(status: number, message: string) {
    super(message);
    this.status = status;
    this.name = "ApiError";
  }
}

/** Only clear the session on 401 for our own admin/login endpoints,
 *  not for provider proxy paths that may forward upstream 401s. */
function isSessionRoute(path: string): boolean {
  return path.startsWith("/admin") || path.startsWith("/login");
}

function handleUnauthorized(path: string): boolean {
  if (isSessionRoute(path)) {
    clearSession();
    window.location.reload();
    return true;
  }
  return false;
}

/** A promise that never resolves — used after triggering a page reload
 *  so callers don't flash error toasts before the browser navigates. */
function hang(): Promise<never> {
  return new Promise(() => {});
}

export async function parseApiError(response: Response): Promise<ApiError> {
  const text = await response.text();
  const payload = parseMaybeJson(text);
  if (payload && typeof payload === "object") {
    const object = payload as Record<string, unknown>;
    if (typeof object.error === "string") {
      return new ApiError(response.status, object.error);
    }
    if (typeof object.message === "string") {
      return new ApiError(response.status, object.message);
    }
  }
  return new ApiError(response.status, text.trim() || `HTTP ${response.status}`);
}

export async function apiJson<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(path, init);
  if (!response.ok) {
    if (response.status === 401 && handleUnauthorized(path)) return hang();
    throw await parseApiError(response);
  }
  const text = await response.text();
  return parseMaybeJson(text) as T;
}

export async function apiText(path: string, init?: RequestInit): Promise<string> {
  const response = await fetch(path, init);
  if (!response.ok) {
    if (response.status === 401 && handleUnauthorized(path)) return hang();
    throw await parseApiError(response);
  }
  return response.text();
}

export async function apiVoid(path: string, init?: RequestInit): Promise<void> {
  const response = await fetch(path, init);
  if (!response.ok) {
    if (response.status === 401 && handleUnauthorized(path)) return hang();
    throw await parseApiError(response);
  }
}

function parseMaybeJson(text: string): unknown {
  const trimmed = text.trim();
  if (!trimmed) {
    return {};
  }
  if (!trimmed.startsWith("{") && !trimmed.startsWith("[")) {
    return trimmed;
  }
  try {
    return JSON.parse(preserveBigTraceIds(trimmed));
  } catch {
    return trimmed;
  }
}

const BIG_ID_KEYS = "trace_id|downstream_trace_id|cursor_trace_id";
const BIG_ID_PARSE_RE = new RegExp(`"(${BIG_ID_KEYS})":\\s*(\\d{16,})`, "g");
const BIG_ID_PARSE_EQ_RE = new RegExp(
  `"(${BIG_ID_KEYS})":\\s*\\{\\s*"Eq"\\s*:\\s*(\\d{16,})\\s*\\}`,
  "g",
);

/// Rewrites big-int trace IDs in JSON text so they survive `JSON.parse`'s
/// IEEE-754 precision loss (anything above 2^53 gets silently rounded,
/// which produced a wrong trace_id on copy/display). 16+ digit integers
/// next to a trace-id key are quoted so they parse as strings.
function preserveBigTraceIds(text: string): string {
  return text
    .replace(BIG_ID_PARSE_EQ_RE, '"$1":{"Eq":"$2"}')
    .replace(BIG_ID_PARSE_RE, '"$1":"$2"');
}

const BIG_ID_STRINGIFY_KEY_RE = new RegExp(
  `"(${BIG_ID_KEYS})":"(\\d{16,})"`,
  "g",
);
const BIG_ID_STRINGIFY_EQ_RE = new RegExp(
  `"(${BIG_ID_KEYS})":\\{"Eq":"(\\d{16,})"\\}`,
  "g",
);
const BIG_ID_ARRAY_RE = /"trace_ids":\s*\[([^\]]*)\]/g;

/// Inverse of `preserveBigTraceIds`: when building a request body for the
/// backend, trace-id fields held as strings on the client must go back out
/// as JSON numbers so serde can deserialize `i64`. Also unwraps string
/// elements inside `trace_ids` arrays for the batch-delete endpoint.
export function stringifyRequest(value: unknown): string {
  const raw = JSON.stringify(value);
  return raw
    .replace(BIG_ID_STRINGIFY_EQ_RE, '"$1":{"Eq":$2}')
    .replace(BIG_ID_STRINGIFY_KEY_RE, '"$1":$2')
    .replace(BIG_ID_ARRAY_RE, (_, inner: string) => {
      const rewritten = inner.replace(/"(\d{16,})"/g, "$1");
      return `"trace_ids":[${rewritten}]`;
    });
}

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
    return JSON.parse(trimmed);
  } catch {
    return trimmed;
  }
}

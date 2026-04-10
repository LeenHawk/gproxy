import { clearSession } from "../app/session";

export class ApiError extends Error {
  status: number;

  constructor(status: number, message: string) {
    super(message);
    this.status = status;
    this.name = "ApiError";
  }
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
    if (response.status === 401) {
      clearSession();
      window.location.reload();
    }
    throw await parseApiError(response);
  }
  const text = await response.text();
  return parseMaybeJson(text) as T;
}

export async function apiText(path: string, init?: RequestInit): Promise<string> {
  const response = await fetch(path, init);
  if (!response.ok) {
    if (response.status === 401) {
      clearSession();
      window.location.reload();
    }
    throw await parseApiError(response);
  }
  return response.text();
}

export async function apiVoid(path: string, init?: RequestInit): Promise<void> {
  const response = await fetch(path, init);
  if (!response.ok) {
    if (response.status === 401) {
      clearSession();
      window.location.reload();
    }
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

export type ApiOptions = {
  method?: string;
  body?: unknown;
  adminKey?: string;
  headers?: Record<string, string>;
};

export type ApiError = {
  status: number;
  message: string;
};

export async function apiRequest<T>(path: string, options: ApiOptions = {}): Promise<T> {
  const headers = new Headers(options.headers ?? {});
  headers.set("Accept", "application/json");
  if (options.adminKey) {
    headers.set("x-admin-key", options.adminKey);
  }

  let body: string | undefined;
  if (options.body !== undefined) {
    headers.set("Content-Type", "application/json");
    body = JSON.stringify(options.body);
  }

  const response = await fetch(path, {
    method: options.method ?? "GET",
    headers,
    body
  });

  const text = await response.text();
  if (!response.ok) {
    const message = text || response.statusText;
    const error: ApiError = { status: response.status, message };
    throw error;
  }

  if (!text) {
    return undefined as T;
  }

  return JSON.parse(text) as T;
}

export function apiErrorMessage(error: unknown): string {
  if (!error) {
    return "Unknown error";
  }
  if (typeof error === "string") {
    return error;
  }
  if (typeof error === "object" && "message" in error) {
    return String((error as { message?: string }).message ?? "Unknown error");
  }
  return "Unknown error";
}

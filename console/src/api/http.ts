export type ApiErrorType =
  | "unauthorized"
  | "bad_request"
  | "not_found"
  | "conflict"
  | "internal"
  | "network";

/** Matches the backend's `{"error":{"message","type"}}` envelope. */
interface ErrorBody {
  error?: { message?: string; type?: string };
}

export class ApiError extends Error {
  constructor(
    public status: number,
    public type: ApiErrorType,
    message: string,
    public retryAfter?: number,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

export async function api<T>(
  path: string,
  init: Omit<RequestInit, "headers"> & { headers?: Record<string, string> } = {},
): Promise<T> {
  let res: Response;
  try {
    res = await fetch(path, {
      credentials: "include",
      ...init,
      headers: {
        ...(typeof init.body === "string" ? { "content-type": "application/json" } : {}),
        ...init.headers,
      },
    });
  } catch {
    throw new ApiError(0, "network", "network error");
  }
  if (res.status === 204) return undefined as T;
  if (!res.ok) {
    const retryAfterRaw = Number(res.headers.get("retry-after"));
    const retryAfter = Number.isFinite(retryAfterRaw) && retryAfterRaw > 0 ? retryAfterRaw : undefined;
    let type: ApiErrorType = "internal";
    let message = res.statusText || `HTTP ${res.status}`;
    try {
      const body = (await res.json()) as ErrorBody;
      if (body.error?.type) type = body.error.type as ApiErrorType;
      if (body.error?.message) message = body.error.message;
    } catch {
      /* non-JSON error body (e.g. 429 from throttle) — keep defaults */
    }
    throw new ApiError(res.status, type, message, retryAfter);
  }
  return (await res.json()) as T;
}

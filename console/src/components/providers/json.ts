export type JsonResult<T> =
  | { ok: true; value: T }
  | { ok: false }

export function parseJson(text: string): JsonResult<unknown> {
  try {
    return { ok: true, value: JSON.parse(text) as unknown }
  } catch {
    return { ok: false }
  }
}

export function parseJsonObject<T extends object>(text: string): JsonResult<T> {
  const parsed = parseJson(text)
  if (!parsed.ok || parsed.value === null || Array.isArray(parsed.value) || typeof parsed.value !== "object") {
    return { ok: false }
  }
  return { ok: true, value: parsed.value as T }
}

export function prettyJson(value: unknown) {
  return value == null ? "" : (JSON.stringify(value, null, 2) ?? "")
}

import type { TlsFingerprintDto } from "@/generated/TlsFingerprintDto"
import { parseJson, type JsonResult } from "@/components/providers/json"

export const DEFAULT_FINGERPRINT = "__default"
export const CUSTOM_FINGERPRINT = "__custom"

export function parseFingerprint(text: string): JsonResult<TlsFingerprintDto | null> {
  if (!text.trim()) return { ok: true, value: null }
  const parsed = parseJson(text)
  if (!parsed.ok) return { ok: false }
  if (parsed.value === null) return { ok: true, value: null }
  if (Array.isArray(parsed.value) || typeof parsed.value !== "object") return { ok: false }
  return { ok: true, value: parsed.value as TlsFingerprintDto }
}

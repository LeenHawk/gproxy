import type { ConfigurationExportDto } from "@/generated/sdk"

/** Match CLI key encodings and send the SDK's canonical standard base64. */
export function normalizeSourceMasterKey(text: string): string | null {
  const value = text.trim()
  if (!value) return null
  if (/^[\da-f]{64}$/i.test(value)) {
    return btoa(value.match(/../g)!.map(byte => String.fromCharCode(Number.parseInt(byte, 16))).join(""))
  }
  const standard = value.replace(/-/g, "+").replace(/_/g, "/")
  if (/^[A-Za-z0-9+/]{43}=?$/.test(standard)) {
    const bytes = atob(standard)
    const encoded = btoa(bytes)
    if (bytes.length === 32 && encoded.replace(/=$/, "") === standard.replace(/=$/, "")) return encoded
  }
  throw new Error("Source master key must be 32 bytes encoded as hex or base64")
}

/** This is a local envelope check, not server-side import validation. */
export function parseConfiguration(text: string): ConfigurationExportDto {
  const value: unknown = JSON.parse(text)
  if (!value || typeof value !== "object" || !("formatVersion" in value) || value.formatVersion !== 4 || !("data" in value) || !value.data || typeof value.data !== "object" || Array.isArray(value.data)) throw new Error("Unsupported configuration document (formatVersion 4 required)")
  return value as ConfigurationExportDto
}

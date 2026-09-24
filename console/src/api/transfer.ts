import type { ConfigurationExportDto } from "@/generated/sdk"
/** This is a local envelope check, not server-side import validation. */
export function parseConfiguration(text: string): ConfigurationExportDto {
  const value: unknown = JSON.parse(text)
  if (!value || typeof value !== "object" || !("formatVersion" in value) || value.formatVersion !== 4 || !("data" in value) || !value.data || typeof value.data !== "object" || Array.isArray(value.data)) throw new Error("Unsupported configuration document (formatVersion 4 required)")
  return value as ConfigurationExportDto
}

export const PRICE_FIELDS = [
  "input_price",
  "output_price",
  "cache_read_price",
  "cache_creation_5m_price",
  "cache_creation_30m_price",
  "cache_creation_1h_price",
  "image_output_price",
] as const

export type TierDraft = {
  serviceTier: string
  threshold: string
  multiplier: string
  prices: Record<(typeof PRICE_FIELDS)[number], string>
}

export function tierDrafts(value: unknown): Array<TierDraft> {
  if (!Array.isArray(value)) return []
  return value.flatMap((item) => {
    if (item == null || typeof item !== "object" || Array.isArray(item)) return []
    const row = item as Record<string, unknown>
    const prices = Object.fromEntries(PRICE_FIELDS.map((field) => [field, text(row[field])])) as TierDraft["prices"]
    return [{
      serviceTier: text(row.service_tier),
      threshold: row.min_prompt_tokens == null ? "" : text(row.min_prompt_tokens),
      multiplier: text(row.multiplier),
      prices,
    }]
  })
}

export function serializeTiers(rows: Array<TierDraft>) {
  return rows.map((row) => {
    const value: Record<string, unknown> = {}
    if (row.serviceTier.trim()) value.service_tier = row.serviceTier.trim()
    if (row.threshold.trim()) value.min_prompt_tokens = Number(row.threshold)
    if (row.multiplier.trim()) value.multiplier = row.multiplier.trim()
    for (const field of PRICE_FIELDS) {
      if (row.prices[field].trim()) value[field] = row.prices[field].trim()
    }
    return value
  })
}

export function losesLongContextStep(rows: Array<TierDraft>) {
  const baseThresholds = rows
    .filter((row) => !row.serviceTier.trim())
    .map((row) => Number(row.threshold || 0))
  return rows.some((row) => {
    const explicit = row.serviceTier.trim() && PRICE_FIELDS.some((field) => row.prices[field].trim())
    return explicit && baseThresholds.some((threshold) => threshold > Number(row.threshold || 0))
  })
}

export function emptyTier(): TierDraft {
  return {
    serviceTier: "",
    threshold: "",
    multiplier: "",
    prices: Object.fromEntries(PRICE_FIELDS.map((field) => [field, ""])) as TierDraft["prices"],
  }
}

function text(value: unknown) {
  return typeof value === "string" || typeof value === "number" ? String(value) : ""
}

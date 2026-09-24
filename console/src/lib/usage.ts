import { formatCount, formatPercent } from "@/lib/format"

export const CACHE_TOKEN_FIELDS = ["cachedInputTokens", "cacheCreation5mTokens", "cacheCreation30mTokens", "cacheCreation1hTokens"] as const

type CacheUsage = {
  inputTokens: number | null
  cachedInputTokens: number | null
  cacheCreationTokens?: number
  cacheCreation5mTokens: number | null
  cacheCreation30mTokens: number | null
  cacheCreation1hTokens: number | null
}

/** Input is normalized uncached input. Cache reads and writes are disjoint from it. */
export function cacheHitRate(tokens: CacheUsage): number | null {
  if (tokens.inputTokens == null || tokens.cachedInputTokens == null) return null
  const writes = tokens.cacheCreationTokens ?? ((tokens.cacheCreation5mTokens ?? 0) + (tokens.cacheCreation30mTokens ?? 0) + (tokens.cacheCreation1hTokens ?? 0))
  const total = tokens.inputTokens + tokens.cachedInputTokens + writes
  return total > 0 ? tokens.cachedInputTokens / total : null
}

export function formatCacheHitRate(tokens: CacheUsage, locale: string) {
  const rate = cacheHitRate(tokens)
  return rate === null ? "—" : formatPercent(rate, locale)
}

/** Unreported counts must stay distinguishable from a measured zero. */
export function formatUsageTokens(value: number | null | undefined, locale: string) {
  return value == null ? "—" : formatCount(value, locale)
}

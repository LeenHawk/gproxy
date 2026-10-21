import { describe, expect, it } from "vitest"
import { cacheHitRate, formatCacheHitRate, formatUsageTokens } from "./usage"

const counts = { inputTokens: 10, cachedInputTokens: 50, cacheCreation: [{ tokens: 10, ttlSeconds: 300 }, { tokens: 20, ttlSeconds: null }, { tokens: 10, ttlSeconds: 43200 }] }
describe("cache hit rate", () => {
  it("includes all cache writes in total input, without double counting the aggregate", () => {
    expect(cacheHitRate(counts)).toBe(0.5)
    expect(cacheHitRate({ ...counts, cacheCreationTokens: 40 })).toBe(0.5)
    expect(formatCacheHitRate(counts, "en")).toBe("50%")
  })
  it("derives a weighted rate from total tokens rather than averaging request percentages", () => {
    expect(cacheHitRate({ ...counts, inputTokens: 910, cachedInputTokens: 90, cacheCreationTokens: 0 })).toBe(0.09)
  })
  it("distinguishes zero hits from absent input or unreported counts", () => {
    expect(cacheHitRate({ ...counts, cachedInputTokens: 0 })).toBe(0)
    expect(cacheHitRate({ ...counts, inputTokens: null })).toBeNull()
    expect(cacheHitRate({ ...counts, cachedInputTokens: null })).toBeNull()
    expect(formatCacheHitRate({ ...counts, inputTokens: 0, cachedInputTokens: 0, cacheCreationTokens: 0 }, "en")).toBe("—")
    expect(formatUsageTokens(null, "en")).toBe("—")
    expect(formatUsageTokens(0, "en")).toBe("0")
  })
})

import { describe, expect, it } from "vitest"
import { defaultPriceFor, matchingModel } from "./model-catalog"
import type { DefaultModelDto } from "@/generated/sdk"
const model = (name: string): DefaultModelDto => ({ modelId: name, displayName: null, contextWindow: null, maxOutputTokens: null, pricing: { modelPattern: `*${name}*`, priority: 1_000_000 - name.length, rates: [], tiers: null } })
describe("default model matching", () => {
  it("uses exact names before unique basenames and ignores case", () => {
    const names = ["shared", "vendor/shared"]
    expect(matchingModel(names, "SHARED", name => name)).toBe("shared")
    expect(matchingModel(names, "VENDOR/SHARED", name => name)).toBe("vendor/shared")
    expect(matchingModel(["vendor/shared"], "shared", name => name)).toBe("vendor/shared")
    expect(matchingModel(["a/shared", "b/shared"], "shared", name => name)).toBeUndefined()
  })
  it("matches upstream prefixes and dated suffixes using the most specific price", () => {
    const models = [model("claude-sonnet-4"), model("claude-sonnet-4.5"), model("claude-sonnet-4.5:batch")]
    expect(defaultPriceFor(models, "ANTHROPIC/CLAUDE-SONNET-4.5:BATCH")?.modelPattern).toBe("*claude-sonnet-4.5:batch*")
    expect(defaultPriceFor(models, "claude-sonnet-4.5-20250929")?.modelPattern).toBe("*claude-sonnet-4.5*")
    expect(defaultPriceFor(models, "claude-sonnet-4/unrelated")).toBeUndefined()
  })
})

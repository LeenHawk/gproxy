import { describe, expect, it } from "vitest"
import { defaultPriceFor, matchingModel, providersByModel } from "./model-catalog"
import type { DefaultModelDto, ProviderDto, ProviderModelDto } from "@/generated/sdk"
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


const provider = (id: string, name: string): ProviderDto => ({ id, name, channel: "codex", baseUrl: null, connectionProfileId: null, proxy: null, config: {}, enabled: true, createdAtMs: 0 })
const binding = (id: string, providerId: string, upstreamName: string, modelId: string | null = null): ProviderModelDto => ({ id, providerId, upstreamName, modelId, metadata: {}, enabled: true })
describe("catalog provider instances", () => {
  it("uses explicit model links and qualified names, listing each instance once", () => {
    const rows = [{ name: "gpt-test", local: { id: "global-1" } }, { name: "unlinked" }]
    const primary = provider("primary", "Office gateway"), backup = provider("backup", "Backup gateway")
    const result = providersByModel(rows, [primary, backup], [
      binding("alias", "primary", "private-model-alias", "global-1"),
      binding("named", "primary", "gpt-test"),
      binding("other", "backup", "openai/GPT-TEST"),
    ])
    expect(result.get(rows[0])?.map(row => row.name)).toEqual(["Backup gateway", "Office gateway"])
    expect(result.has(rows[1])).toBe(false)
  })
  it("does not infer providers from an unresolved explicit link or an ambiguous basename", () => {
    const rows = [{ name: "one/shared" }, { name: "two/shared" }, { name: "gpt-test" }]
    expect(providersByModel(rows, [provider("p", "Gateway")], [
      binding("missing", "p", "gpt-test", "missing-global-id"),
      binding("ambiguous", "p", "shared"),
    ]).size).toBe(0)
  })
})

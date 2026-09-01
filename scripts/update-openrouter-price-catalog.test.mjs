import assert from "node:assert/strict"
import test from "node:test"

import { buildCatalog, perMillion } from "./update-openrouter-price-catalog.mjs"

test("converts exact OpenRouter token decimals to per-million prices", () => {
  assert.equal(perMillion("0.00000625"), "6.25")
  assert.equal(perMillion("0.0000000833333333333333"), "0.0833333333333333")
  assert.equal(perMillion("0"), "0")
})

test("maps v3 usage metrics and omits dynamic and unsupported price units", () => {
  const catalog = buildCatalog({
    total_count: 5,
    data: [
      {
        id: "openai/gpt-test",
        architecture: { output_modalities: ["text"] },
        pricing: {
          prompt: "0.000001",
          completion: "0.000002",
          input_cache_read: "0.0000001",
          input_cache_write: "0.00000125",
          web_search: "0.005",
          overrides: [{ min_prompt_tokens: 200000, prompt: "0.000002" }],
        },
      },
      {
        id: "anthropic/claude-test",
        architecture: { output_modalities: ["text"] },
        pricing: { prompt: "0.000003", completion: "0.000015", input_cache_write: "0.00000375" },
      },
      {
        id: "openrouter/auto",
        architecture: { output_modalities: ["text"] },
        pricing: { prompt: "-1", completion: "-1" },
      },
      {
        id: "deepseek/time-priced",
        architecture: { output_modalities: ["text"] },
        pricing: {
          prompt: "0.000001",
          completion: "0.000002",
          overrides: [{ utc_start: 100, utc_end: 200, prompt: "0.000002" }],
        },
      },
      {
        id: "openai/whisper-test",
        architecture: { output_modalities: ["transcription"] },
        pricing: { prompt: "0.006", completion: "0" },
      },
    ],
  }, "2026-09-02T00:00:00.000Z")
  assert.equal(catalog.source.supported_output_models, 4)
  assert.equal(catalog.source.dynamic_price_models, 2)
  assert.deepEqual(catalog.price_rules.map((rule) => rule.model_id), [
    "anthropic/claude-test",
    "openai/gpt-test",
  ])
  const claude = catalog.price_rules[0]
  assert(claude.rates.some((rate) =>
    rate.metric === "cache_creation_5m_tokens" && rate.price === "3.75"))
  const openai = catalog.price_rules[1]
  assert.deepEqual(openai.tiers, [{ min_prompt_tokens: 200000, input_price: "2" }])
  assert(openai.rates.some((rate) =>
    rate.metric === "cache_creation_30m_tokens" && rate.unit_size === 1_000_000))
  assert(openai.rates.some((rate) =>
    rate.metric === "web_searches" && rate.unit_size === 1 && rate.price === "0.005"))
})

test("rejects duplicate global basename patterns", () => {
  const model = (id) => ({
    id,
    architecture: { output_modalities: ["text"] },
    pricing: { prompt: "0", completion: "0" },
  })
  assert.throws(
    () => buildCatalog({ data: [model("one/shared"), model("two/shared")] }),
    /duplicate OpenRouter model basename/,
  )
})

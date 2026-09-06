import assert from "node:assert/strict"
import test from "node:test"

import { applyCodexCatalog, buildCatalog, perMillion } from "./update-openrouter-model-catalog.mjs"

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
        context_length: 128000,
        top_provider: { max_completion_tokens: 32000 },
        supported_parameters: ["tools", "reasoning"],
        architecture: { input_modalities: ["text"], output_modalities: ["text"] },
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
        context_length: 200000,
        top_provider: { max_completion_tokens: 64000 },
        supported_parameters: ["tools"],
        architecture: { input_modalities: ["text", "image"], output_modalities: ["text"] },
        pricing: { prompt: "0.000003", completion: "0.000015", input_cache_write: "0.00000375" },
      },
      {
        id: "openrouter/auto",
        context_length: 1000000,
        top_provider: { max_completion_tokens: null },
        supported_parameters: [],
        architecture: { input_modalities: ["text"], output_modalities: ["text"] },
        pricing: { prompt: "-1", completion: "-1" },
      },
      {
        id: "deepseek/time-priced",
        context_length: 64000,
        top_provider: { max_completion_tokens: 8000 },
        supported_parameters: [],
        architecture: { input_modalities: ["text"], output_modalities: ["text"] },
        pricing: {
          prompt: "0.000001",
          completion: "0.000002",
          overrides: [{ utc_start: 100, utc_end: 200, prompt: "0.000002" }],
        },
      },
      {
        id: "openai/whisper-test",
        context_length: 32000,
        top_provider: { max_completion_tokens: 4000 },
        supported_parameters: [],
        architecture: { input_modalities: ["audio"], output_modalities: ["transcription"] },
        pricing: { prompt: "0.006", completion: "0" },
      },
    ],
  }, "2026-09-02T00:00:00.000Z")
  assert.equal(catalog.models.length, 5)
  assert.equal(catalog.source.priced_models, 2)
  assert.equal(catalog.source.dynamic_price_models, 2)
  assert.deepEqual(catalog.models.filter((model) => model.pricing).map((model) => model.model_id), [
    "anthropic/claude-test",
    "openai/gpt-test",
  ])
  const claude = catalog.models[0]
  assert.equal(claude.context_window, 200000)
  assert.equal(claude.max_output_tokens, 64000)
  assert.deepEqual(claude.input_modalities, ["image", "text"])
  assert(claude.pricing.rates.some((rate) =>
    rate.metric === "cache_creation_5m_tokens" && rate.price === "3.75"))
  const openai = catalog.models.find((model) => model.model_id === "openai/gpt-test")
  assert.deepEqual(openai.pricing.tiers, [{ min_prompt_tokens: 200000, input_price: "2" }])
  assert(openai.pricing.rates.some((rate) =>
    rate.metric === "cache_creation_30m_tokens" && rate.unit_size === 1_000_000))
  assert(openai.pricing.rates.some((rate) =>
    rate.metric === "web_searches" && rate.unit_size === 1 && rate.price === "0.005"))
  assert.equal(catalog.models.find((model) => model.model_id === "openrouter/auto").pricing, null)
  assert.equal(catalog.models.find((model) => model.model_id === "openai/whisper-test").pricing, null)
})

test("rejects duplicate global basename patterns", () => {
  const model = (id) => ({
    id,
    context_length: 1000,
    top_provider: { max_completion_tokens: 100 },
    supported_parameters: [],
    architecture: { input_modalities: ["text"], output_modalities: ["text"] },
    pricing: { prompt: "0", completion: "0" },
  })
  assert.throws(
    () => buildCatalog({ data: [model("one/shared"), model("two/shared")] }),
    /duplicate OpenRouter model basename/,
  )
})

test("overlays Codex capabilities and adds Codex-only models", () => {
  const catalog = buildCatalog({ data: [{
    id: "openai/gpt-test",
    context_length: 1000,
    top_provider: { max_completion_tokens: 100 },
    supported_parameters: ["tools"],
    architecture: { input_modalities: ["text"], output_modalities: ["text"] },
    pricing: { prompt: "0", completion: "0" },
  }] }, "2026-09-06T00:00:00.000Z")
  applyCodexCatalog(catalog, { models: [{
    slug: "gpt-test",
    display_name: "GPT Test",
    base_instructions: "Use tools carefully.",
    context_window: 2000,
    supported_reasoning_levels: [{ effort: "high", description: "Deep" }],
    service_tiers: [],
  }, { slug: "codex-only", input_modalities: ["text"] }] }, "abc123")
  const model = catalog.models.find((entry) => entry.model_id === "openai/gpt-test")
  assert.equal(model.context_window, 2000)
  assert.equal(model.instructions, "Use tools carefully.")
  assert.equal(model.supported_reasoning_levels[0].effort, "high")
  assert(model.supported_parameters.includes("reasoning_effort"))
  assert(catalog.models.some((entry) => entry.model_id === "openai/codex-only"))
  assert.equal(catalog.source.codex_revision, "abc123")
})

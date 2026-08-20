import { readFile } from "node:fs/promises";
import { describe, expect, it } from "vitest";
import {
  applyOfficialPriceOverrides,
  buildPriceBundle,
  perMillion,
} from "./update-openrouter-price-rules.mjs";

const bundledRulesUrl = new URL("../src/data/openrouter-price-rules.json", import.meta.url);

describe("OpenRouter price-rule generator", () => {
  it("converts token decimals to exact per-million prices", () => {
    expect(perMillion("0.00000625")).toBe("6.25");
    expect(perMillion("0.0000000833333333333333")).toBe("0.0833333333333333");
    expect(perMillion("0")).toBe("0");
  });

  it("keeps priced variants, maps cache TTLs, and omits dynamic prices", () => {
    const bundle = buildPriceBundle({
      total_count: 6,
      data: [
        {
          id: "openai/gpt-test:batch",
          architecture: { output_modalities: ["text"] },
          pricing: {
            prompt: "0.000001",
            completion: "0.000002",
            input_cache_read: "0.0000001",
            input_cache_write: "0.00000125",
          },
        },
        {
          id: "anthropic/claude-test",
          architecture: { output_modalities: ["text"] },
          pricing: {
            prompt: "0.000003",
            completion: "0.000015",
            input_cache_write: "0.00000375",
            input_cache_write_1h: "0.000006",
          },
        },
        {
          id: "~openai/gpt-latest",
          architecture: { output_modalities: ["text"] },
          pricing: {
            prompt: "0.000001",
            completion: "0.000002",
            input_cache_write: "0.00000125",
          },
        },
        {
          id: "openrouter/auto",
          architecture: { output_modalities: ["text", "image"] },
          pricing: { prompt: "-1", completion: "-1" },
        },
        {
          id: "qwen/image-test",
          architecture: { output_modalities: ["image"] },
          pricing: {
            prompt: "0",
            completion: "0",
            image: "123",
            image_output: "0.0000075",
          },
        },
        {
          id: "voyage/embedding-test",
          architecture: { output_modalities: ["embeddings"] },
          pricing: { prompt: "0.00000012", completion: "0" },
        },
      ],
    });

    expect(bundle.price_rules.map((rule) => rule.model_match)).toEqual([
      "claude-test",
      "embedding-test",
      "gpt-latest",
      "gpt-test:batch",
      "image-test",
    ]);
    expect(bundle.schema_version).toBe(2);
    expect(bundle.source).toMatchObject({
      total_models: 6,
      supported_output_models: 6,
      dynamic_price_models: 1,
      included_models: 5,
      embedding_models: 1,
      rerank_models: 0,
      image_output_priced_models: 1,
    });
    expect(bundle.price_rules[0]).toMatchObject({
      cache_creation_5m_price: "3.75",
      cache_creation_30m_price: "0",
      cache_creation_1h_price: "6",
      image_output_price: "0",
    });
    expect(bundle.price_rules[1]).toMatchObject({
      input_price: "0.12",
      output_price: "0",
    });
    expect(bundle.price_rules[2]).toMatchObject({
      cache_creation_5m_price: "0",
      cache_creation_30m_price: "1.25",
    });
    expect(bundle.price_rules[3]).toMatchObject({
      input_price: "1",
      output_price: "2",
      cache_read_price: "0.1",
      cache_creation_30m_price: "1.25",
    });
    expect(bundle.price_rules[3].rates).toEqual(expect.arrayContaining([
      expect.objectContaining({ metric: "input_tokens", unit: "token", price_usd: "0.000001" }),
      expect.objectContaining({ metric: "cache_creation_30m_tokens", price_usd: "0.00000125" }),
    ]));
    expect(bundle.price_rules[4].rates).toEqual(expect.arrayContaining([
      expect.objectContaining({ metric: "image_inputs", unit: "image", price_usd: "123" }),
      expect.objectContaining({ metric: "image_output_tokens", price_usd: "0.0000075" }),
    ]));
    expect(bundle.price_rules[4]).toMatchObject({
      input_price: "0",
      output_price: "0",
      image_output_price: "7.5",
    });
  });

  it("keeps every supported output modality and excludes unsupported-only dimensions", () => {
    const model = (id, outputModalities) => ({
      id,
      architecture: { output_modalities: outputModalities },
      pricing: { prompt: "0.000001", completion: "0.000002" },
    });
    const bundle = buildPriceBundle({
      data: [
        model("openai/audio-text", ["audio", "text"]),
        model("audio/speech", ["speech"]),
        model("video/generator", ["video"]),
        model("audio/transcriber", ["transcription"]),
        model("search/reranker", ["rerank"]),
      ],
    });

    expect(bundle.price_rules.map((rule) => rule.model_match)).toEqual([
      "audio-text",
      "reranker",
    ]);
    expect(bundle.source.rerank_models).toBe(1);
  });

  it("rejects duplicate basenames", () => {
    const model = (id) => ({
      id,
      architecture: { output_modalities: ["text"] },
      pricing: { prompt: "0", completion: "0" },
    });
    expect(() => buildPriceBundle({ data: [model("one/shared"), model("two/shared")] }))
      .toThrow("duplicate OpenRouter model basename");
  });

  it("applies current provider-published overrides", () => {
    const bundle = applyOfficialPriceOverrides(buildPriceBundle({
      data: [{
        id: "deepseek/deepseek-v4-flash",
        architecture: { output_modalities: ["text"] },
        pricing: { prompt: "0.00000014", completion: "0.00000028", input_cache_read: "0.000000028" },
      }],
    }));
    expect(bundle.source.catalog).toBe("openrouter+official-overrides");
    expect(bundle.price_rules.find((rule) => rule.model_match === "deepseek-v4-flash")?.cache_read_price).toBe("0.0028");
    expect(bundle.price_rules.find((rule) => rule.model_match === "grok-4.6")).toMatchObject({
      input_price: "2",
      output_price: "6",
      cache_read_price: "0.5",
      pricing_tiers_json: [{
        min_prompt_tokens: 200000,
        input_price: "4",
        output_price: "12",
        cache_read_price: "1",
      }, {
        service_tier: "priority",
        multiplier: "2",
      }],
    });
    expect(bundle.price_rules.find((rule) => rule.model_match === "gemini-3.7-flash")).toMatchObject({
      input_price: "0.75",
      output_price: "3.75",
      cache_read_price: "0.075",
      pricing_tiers_json: [
        { service_tier: "flex", input_price: "0.375", output_price: "1.875" },
        { service_tier: "priority", input_price: "1.35", output_price: "6.75" },
      ],
    });
  });

  it("keeps the checked-in full catalog complete, sorted, and aligned with the generator", async () => {
    const bundle = JSON.parse(await readFile(bundledRulesUrl, "utf8"));
    const generatedShape = applyOfficialPriceOverrides(buildPriceBundle({
      data: [{
        id: "test/shape",
        architecture: { output_modalities: ["text"] },
        pricing: { prompt: "0", completion: "0" },
      }],
    }));
    const baseShape = buildPriceBundle({
      data: [{
        id: "test/shape",
        architecture: { output_modalities: ["text"] },
        pricing: { prompt: "0", completion: "0" },
      }],
    }).price_rules[0];
    const expectedFields = Object.keys(baseShape);
    const priceFields = expectedFields.filter((field) => field.endsWith("_price"));
    const names = bundle.price_rules.map((rule) => rule.model_match);

    expect(bundle.schema_version).toBe(generatedShape.schema_version);
    expect(bundle.source).toEqual({
      catalog: "openrouter+official-overrides",
      total_models: 547,
      supported_output_models: 487,
      dynamic_price_models: 5,
      included_models: 482,
      embedding_models: 34,
      rerank_models: 7,
      image_output_priced_models: 43,
    });
    expect(bundle.price_rules).toHaveLength(482);
    expect(new Set(names).size).toBe(names.length);
    expect(names).toEqual([...names].sort());
    expect(bundle.price_rules.filter((rule) => rule.image_output_price !== "0")).toHaveLength(43);
    expect(bundle.price_rules.filter((rule) => rule.pricing_tiers_json != null)).toHaveLength(86);
    expect(bundle.price_rules.filter((rule) => rule.model_match.includes("rerank"))).toHaveLength(7);
    const byName = new Map(bundle.price_rules.map((rule) => [rule.model_match, rule]));
    expect(byName.get("gpt-5.6-terra")).toMatchObject({
      input_price: "2",
      output_price: "12",
      pricing_tiers_json: expect.arrayContaining([
        { service_tier: "flex", multiplier: "0.5" },
        { service_tier: "priority", multiplier: "2" },
      ]),
    });
    expect(byName.get("claude-opus-5")?.pricing_tiers_json).toContainEqual({
      service_tier: "priority",
      multiplier: "2",
    });
    expect(byName.get("gemini-3.7-flash")?.pricing_tiers_json).toContainEqual({
      service_tier: "priority",
      input_price: "1.35",
      output_price: "6.75",
      cache_read_price: "0.135",
    });
    expect(bundle.price_rules.every((rule) => (
      expectedFields.filter((field) => field !== "rates").every((field) => field in rule)
      && Object.keys(rule).every((field) => (
        expectedFields.includes(field)
        || field === "pricing_tiers_json"
        || field === "rates"
      ))
    ))).toBe(true);
    expect(bundle.price_rules.every((rule) => (
      rule.provider_id === null
      && rule.match_type === "contains"
      && typeof rule.model_match === "string"
      && rule.model_match.length > 0
      && rule.enabled === true
      && priceFields.every((field) => (
        typeof rule[field] === "string" && /^\d+(?:\.\d+)?$/.test(rule[field])
      ))
    ))).toBe(true);
    expect(bundle.price_rules.some((rule) => "image_price" in rule)).toBe(false);
  });
});

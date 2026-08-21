#!/usr/bin/env node

import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const MODELS_URL = "https://openrouter.ai/api/v1/models?limit=1000&output_modalities=all";
const scriptPath = fileURLToPath(import.meta.url);
const consoleDir = path.resolve(path.dirname(scriptPath), "..");
const outputPath = path.join(consoleDir, "src", "data", "openrouter-price-rules.json");
const DECIMAL = /^(-?)(\d+)(?:\.(\d+))?$/;
const SUPPORTED_OUTPUT_MODALITIES = new Set(["text", "image", "embeddings", "rerank"]);
const OFFICIAL_RULE_OVERRIDES = new Map([
  ["deepseek-v4-flash", { cache_read_price: "0.0028" }],
  ["gpt-5.6-sol", {
    input_price: "5", output_price: "30", cache_read_price: "0.5",
    cache_creation_30m_price: "6.25",
    pricing_tiers_json: [{
      min_prompt_tokens: 272000,
      input_price: "10", output_price: "45", cache_read_price: "1",
      cache_creation_30m_price: "12.5",
    }],
  }],
  ["gpt-5.6-terra", {
    input_price: "2", output_price: "12", cache_read_price: "0.2",
    cache_creation_30m_price: "2.5",
    pricing_tiers_json: [{
      min_prompt_tokens: 272000,
      input_price: "4", output_price: "18", cache_read_price: "0.4",
      cache_creation_30m_price: "5",
    }],
  }],
  ["gpt-5.6-luna", {
    input_price: "0.2", output_price: "1.2", cache_read_price: "0.02",
    cache_creation_30m_price: "0.25",
    pricing_tiers_json: [{
      min_prompt_tokens: 272000,
      input_price: "0.4", output_price: "1.8", cache_read_price: "0.04",
      cache_creation_30m_price: "0.5",
    }],
  }],
  ["gemini-3.7-flash", {
    input_price: "0.75", output_price: "3.75", cache_read_price: "0.075",
  }],
  ["gemini-3.6-flash", {
    input_price: "0.75", output_price: "3.75", cache_read_price: "0.075",
  }],
  ["gemini-3.5-flash", {
    input_price: "1.5", output_price: "9", cache_read_price: "0.15",
  }],
  ["gemini-3.5-flash-lite", {
    input_price: "0.3", output_price: "2.5", cache_read_price: "0.03",
  }],
]);

const multiplierTier = (serviceTier, multiplier) => ({
  service_tier: serviceTier,
  multiplier,
});
const pricedTier = (serviceTier, input, output, cacheRead, minPromptTokens) => ({
  service_tier: serviceTier,
  ...(minPromptTokens == null ? {} : { min_prompt_tokens: minPromptTokens }),
  input_price: input,
  output_price: output,
  cache_read_price: cacheRead,
});
const flexAndPriority = (priorityMultiplier) => [
  multiplierTier("flex", "0.5"),
  multiplierTier("priority", priorityMultiplier),
];

// Provider-published service-tier prices as of 2026-08-15. These stay
// model-specific: Fast/Priority is not one universal multiplier.
const OFFICIAL_SERVICE_TIER_OVERRIDES = new Map([
  ...["gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna"].map((model) => (
    [model, flexAndPriority("2")]
  )),
  ["gpt-5.5", flexAndPriority("2.5")],
  ["gpt-5.5-pro", [multiplierTier("flex", "0.5")]],
  ["gpt-5.4", flexAndPriority("2")],
  ["gpt-5.4-mini", flexAndPriority("2")],
  ["gpt-5.4-nano", [multiplierTier("flex", "0.5")]],
  ["gpt-5.4-pro", [multiplierTier("flex", "0.5")]],
  ["gpt-5.2", flexAndPriority("2")],
  ["gpt-5.1", flexAndPriority("2")],
  ["gpt-5", flexAndPriority("2")],
  ["gpt-5-mini", flexAndPriority("1.8")],
  ["gpt-5-nano", [multiplierTier("flex", "0.5")]],
  ["gpt-4.1", [multiplierTier("priority", "1.75")]],
  ["gpt-4.1-mini", [multiplierTier("priority", "1.75")]],
  ["gpt-4.1-nano", [multiplierTier("priority", "2")]],
  ["gpt-4o", [multiplierTier("priority", "1.7")]],
  ["gpt-4o-2024-05-13", [multiplierTier("priority", "1.75")]],
  ["gpt-4o-2024-08-06", [multiplierTier("priority", "1.7")]],
  ["gpt-4o-2024-11-20", [multiplierTier("priority", "1.7")]],
  ["gpt-4o-mini", [pricedTier("priority", "0.25", "1", "0.125")]],
  ["o3", flexAndPriority("1.75")],
  ["o4-mini", flexAndPriority("1.818181818181818181818181818")],
  ["claude-opus-4.8", [multiplierTier("priority", "2")]],
  ["claude-opus-5", [multiplierTier("priority", "2")]],
  ["gemini-3.7-flash", [
    pricedTier("flex", "0.375", "1.875", "0.0375"),
    pricedTier("priority", "1.35", "6.75", "0.135"),
  ]],
  ["gemini-3.6-flash", [
    pricedTier("flex", "0.375", "1.875", "0.0375"),
    pricedTier("priority", "1.35", "6.75", "0.135"),
  ]],
  ["gemini-3.5-flash", [
    pricedTier("flex", "0.75", "4.5", "0.08"),
    pricedTier("priority", "2.7", "16.2", "0.27"),
  ]],
  ["gemini-3.5-flash-lite", [
    pricedTier("flex", "0.15", "1.25", "0.02"),
    pricedTier("priority", "0.54", "4.5", "0.05"),
  ]],
  ["gemini-3.1-flash-lite", [
    pricedTier("flex", "0.125", "0.75", "0.0125"),
    pricedTier("priority", "0.45", "2.7", "0.045"),
  ]],
  ["gemini-3.1-pro-preview", [
    pricedTier("flex", "1", "6", "0.2"),
    pricedTier("flex", "2", "9", "0.4", 200000),
    pricedTier("priority", "3.6", "21.6", "0.36"),
    pricedTier("priority", "7.2", "32.4", "0.72", 200000),
  ]],
  ["gemini-3.1-pro-preview-customtools", [
    pricedTier("flex", "1", "6", "0.2"),
    pricedTier("flex", "2", "9", "0.4", 200000),
    pricedTier("priority", "3.6", "21.6", "0.36"),
    pricedTier("priority", "7.2", "32.4", "0.72", 200000),
  ]],
  ["gemini-3-flash-preview", [
    pricedTier("flex", "0.25", "1.5", "0.05"),
    pricedTier("priority", "0.9", "5.4", "0.09"),
  ]],
  ["gemini-2.5-pro", [
    pricedTier("flex", "0.625", "5", "0.125"),
    pricedTier("flex", "1.25", "7.5", "0.25", 200000),
    pricedTier("priority", "2.25", "18", "0.225"),
    pricedTier("priority", "4.5", "27", "0.45", 200000),
  ]],
  ["gemini-2.5-flash", [
    pricedTier("flex", "0.15", "1.25", "0.03"),
    pricedTier("priority", "0.54", "4.5", "0.054"),
  ]],
  ["gemini-2.5-flash-lite", [
    pricedTier("flex", "0.05", "0.2", "0.01"),
    pricedTier("priority", "0.18", "0.72", "0.018"),
  ]],
  ...[
    "grok-4.6",
    "grok-4.5",
    "grok-4.3",
    "grok-4.20-0309-reasoning",
    "grok-4.20-0309-non-reasoning",
    "grok-4.20-multi-agent-0309",
    "grok-4.20",
    "grok-4.20-multi-agent",
    "grok-build-0.1",
    "grok-latest",
  ].map((model) => [model, [multiplierTier("priority", "2")]]),
]);

const OFFICIAL_EXTRA_RULES = [
  {
    provider_id: null,
    match_type: "contains",
    model_match: "grok-4.6",
    input_price: "2",
    output_price: "6",
    cache_read_price: "0.5",
    cache_creation_5m_price: "0",
    cache_creation_30m_price: "0",
    cache_creation_1h_price: "0",
    image_output_price: "0",
    pricing_tiers_json: [{
      min_prompt_tokens: 200000,
      input_price: "4",
      output_price: "12",
      cache_read_price: "1",
    }],
    enabled: true,
  },
  {
    provider_id: null,
    match_type: "contains",
    model_match: "gemini-3.7-flash",
    input_price: "0.75",
    output_price: "3.75",
    cache_read_price: "0.075",
    cache_creation_5m_price: "0",
    cache_creation_30m_price: "0",
    cache_creation_1h_price: "0",
    image_output_price: "0",
    pricing_tiers_json: null,
    enabled: true,
  },
];

/** Convert an OpenRouter USD/token decimal into GPROXY's USD/1M-token form. */
export function perMillion(value) {
  if (value == null) return "0";
  if (typeof value !== "string") throw new Error(`price must be a string, got ${typeof value}`);

  const match = DECIMAL.exec(value);
  if (!match) throw new Error(`invalid decimal price: ${value}`);
  if (match[1] === "-") throw new Error(`negative price is not representable: ${value}`);

  const fraction = match[3] ?? "";
  const coefficient = BigInt(`${match[2]}${fraction}`);
  const scale = fraction.length - 6;
  if (coefficient === 0n) return "0";
  if (scale <= 0) return (coefficient * (10n ** BigInt(-scale))).toString();

  const digits = coefficient.toString().padStart(scale + 1, "0");
  const integer = digits.slice(0, -scale);
  const decimal = digits.slice(-scale).replace(/0+$/, "");
  return decimal ? `${integer}.${decimal}` : integer;
}

function metricRate(metric, unit, value, sortOrder) {
  if (value == null || value === "0") return null;
  if (typeof value !== "string" || !DECIMAL.test(value)) {
    throw new Error(`invalid ${metric} price: ${String(value)}`);
  }
  return {
    metric,
    unit,
    unit_size: 1,
    price_usd: value,
    conditions_json: null,
    sort_order: sortOrder,
  };
}

function modelRates(pricing, author) {
  return [
    metricRate("input_tokens", "token", pricing.prompt, 0),
    metricRate("output_tokens", "token", pricing.completion, 1),
    metricRate("cache_read_tokens", "token", pricing.input_cache_read, 2),
    metricRate(author === "openai" ? "cache_creation_30m_tokens" : "cache_creation_5m_tokens", "token", pricing.input_cache_write, 3),
    metricRate("cache_creation_1h_tokens", "token", pricing.input_cache_write_1h, 4),
    metricRate("image_output_tokens", "token", pricing.image_output, 5),
    metricRate("audio_input_tokens", "token", pricing.audio, 6),
    metricRate("audio_output_tokens", "token", pricing.audio_output, 7),
    metricRate("cached_audio_input_tokens", "token", pricing.input_audio_cache, 8),
    metricRate("image_inputs", "image", pricing.image, 10),
    metricRate("web_searches", "request", pricing.web_search, 11),
    metricRate("request", "request", pricing.request, 12),
  ].filter(Boolean);
}

export function buildPriceBundle(payload) {
  if (!payload || !Array.isArray(payload.data)) {
    throw new Error("OpenRouter response does not contain a data array");
  }
  if (Number.isInteger(payload.total_count) && payload.total_count !== payload.data.length) {
    throw new Error(
      `OpenRouter response is incomplete: received ${payload.data.length} of ${payload.total_count}`,
    );
  }

  const rules = [];
  let supportedOutputModels = 0;
  let dynamicPriceModels = 0;
  let embeddingModels = 0;
  let rerankModels = 0;
  for (const model of payload.data) {
    if (typeof model?.id !== "string" || !model.id.includes("/")) {
      throw new Error(`invalid OpenRouter model id: ${String(model?.id)}`);
    }
    const outputModalities = model?.architecture?.output_modalities;
    if (
      !Array.isArray(outputModalities)
      || !outputModalities.some((modality) => SUPPORTED_OUTPUT_MODALITIES.has(modality))
    ) {
      continue;
    }
    supportedOutputModels += 1;
    const pricing = model.pricing;
    if (!pricing || typeof pricing.prompt !== "string" || typeof pricing.completion !== "string") {
      throw new Error(`model ${model.id} has no prompt/completion pricing`);
    }

    // OpenRouter uses -1 for dynamic router prices. A flat default rule cannot
    // represent those safely, so omit the model rather than treating it as free.
    const representedPrices = [
      pricing.prompt,
      pricing.completion,
      pricing.input_cache_read,
      pricing.input_cache_write,
      pricing.input_cache_write_1h,
      pricing.image_output,
    ];
    if (representedPrices.some((price) => typeof price === "string" && price.startsWith("-"))) {
      dynamicPriceModels += 1;
      continue;
    }
    if (outputModalities.includes("embeddings")) embeddingModels += 1;
    if (outputModalities.includes("rerank")) rerankModels += 1;

    const author = model.id.slice(0, model.id.indexOf("/")).replace(/^~/, "");
    const modelMatch = model.id.slice(model.id.lastIndexOf("/") + 1);
    const cacheWrite = perMillion(pricing.input_cache_write);
    const pricingTiers = Array.isArray(pricing.overrides)
      ? pricing.overrides.map((override) => {
          if (!Number.isInteger(override.min_prompt_tokens) || override.min_prompt_tokens <= 0) {
            throw new Error(`model ${model.id} has an invalid pricing override threshold`);
          }
          const tier = { min_prompt_tokens: override.min_prompt_tokens };
          const assign = (field, value) => {
            if (value != null) tier[field] = perMillion(value);
          };
          assign("input_price", override.prompt);
          assign("output_price", override.completion);
          assign("cache_read_price", override.input_cache_read);
          const tierCacheWrite = override.input_cache_write;
          assign(author === "openai" ? "cache_creation_30m_price" : "cache_creation_5m_price", tierCacheWrite);
          assign("cache_creation_1h_price", override.input_cache_write_1h);
          assign("image_output_price", override.image_output);
          return tier;
        })
      : null;
    rules.push({
      provider_id: null,
      match_type: "contains",
      model_match: modelMatch,
      input_price: perMillion(pricing.prompt),
      output_price: perMillion(pricing.completion),
      cache_read_price: perMillion(pricing.input_cache_read),
      // GPROXY classifies OpenAI cache writes as request-wide 30-minute writes.
      // Other providers use OpenRouter's default (normally 5-minute) bucket.
      cache_creation_5m_price: author === "openai" ? "0" : cacheWrite,
      cache_creation_30m_price: author === "openai" ? cacheWrite : "0",
      cache_creation_1h_price: perMillion(pricing.input_cache_write_1h),
      // `image_output` is an output-token rate. Do not use `image`, which is an
      // input-image or flat-image rate depending on the upstream model.
      image_output_price: perMillion(pricing.image_output),
      pricing_tiers_json: pricingTiers?.length ? pricingTiers : null,
      rates: modelRates(pricing, author),
      enabled: true,
    });
  }

  rules.sort((left, right) =>
    left.model_match < right.model_match ? -1 : left.model_match > right.model_match ? 1 : 0,
  );
  for (let index = 1; index < rules.length; index += 1) {
    if (rules[index - 1].model_match === rules[index].model_match) {
      throw new Error(`duplicate OpenRouter model basename: ${rules[index].model_match}`);
    }
  }

  return {
    schema_version: 2,
    source: {
      catalog: "openrouter",
      total_models: payload.data.length,
      supported_output_models: supportedOutputModels,
      dynamic_price_models: dynamicPriceModels,
      included_models: rules.length,
      embedding_models: embeddingModels,
      rerank_models: rerankModels,
      image_output_priced_models: rules.filter((rule) => rule.image_output_price !== "0").length,
    },
    price_rules: rules,
  };
}

/** Apply pricing published directly by providers after the OpenRouter snapshot. */
export function applyOfficialPriceOverrides(bundle) {
  const rules = bundle.price_rules.map((rule) => {
    const overridden = {
      ...rule,
      ...(OFFICIAL_RULE_OVERRIDES.get(rule.model_match) ?? {}),
    };
    const serviceTiers = OFFICIAL_SERVICE_TIER_OVERRIDES.get(rule.model_match);
    if (!serviceTiers) return overridden;
    return {
      ...overridden,
      pricing_tiers_json: [
        ...(overridden.pricing_tiers_json ?? []),
        ...serviceTiers,
      ],
    };
  });
  for (const rule of OFFICIAL_EXTRA_RULES) {
    if (!rules.some((candidate) => candidate.model_match === rule.model_match)) {
      const serviceTiers = OFFICIAL_SERVICE_TIER_OVERRIDES.get(rule.model_match) ?? [];
      rules.push({
        ...rule,
        pricing_tiers_json: [...(rule.pricing_tiers_json ?? []), ...serviceTiers],
      });
    }
  }
  rules.sort((left, right) => (
    left.model_match < right.model_match ? -1 : left.model_match > right.model_match ? 1 : 0
  ));
  return {
    ...bundle,
    source: {
      ...bundle.source,
      catalog: "openrouter+official-overrides",
      included_models: rules.length,
    },
    price_rules: rules,
  };
}

async function loadPayload(inputPath) {
  if (inputPath) {
    return JSON.parse(await readFile(path.resolve(process.cwd(), inputPath), "utf8"));
  }

  const headers = {
    Accept: "application/json",
    "User-Agent": "gproxy-price-table-updater",
  };
  const apiKey = process.env.OPENROUTER_API_KEY?.trim();
  if (apiKey) headers.Authorization = `Bearer ${apiKey}`;

  const response = await fetch(MODELS_URL, { headers });
  if (!response.ok) throw new Error(`OpenRouter models request returned HTTP ${response.status}`);
  return response.json();
}

async function main() {
  const args = process.argv.slice(2);
  if (args.length !== 0 && (args.length !== 2 || args[0] !== "--input")) {
    throw new Error("usage: node scripts/update-openrouter-price-rules.mjs [--input response.json]");
  }

  const payload = await loadPayload(args[1]);
  const bundle = applyOfficialPriceOverrides(buildPriceBundle(payload));
  await writeFile(outputPath, `${JSON.stringify(bundle, null, 2)}\n`, "utf8");

  const variants = bundle.price_rules.filter((rule) => rule.model_match.includes(":")).length;
  const omitted = payload.data.length - bundle.price_rules.length;
  console.log(
    `Updated ${outputPath} with ${bundle.price_rules.length} rules `
      + `(${variants} variants, ${omitted} unsupported or dynamic-price models omitted).`,
  );
}

if (process.argv[1] && path.resolve(process.argv[1]) === scriptPath) {
  try {
    await main();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}

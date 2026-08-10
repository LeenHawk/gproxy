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
  const bundle = buildPriceBundle(payload);
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

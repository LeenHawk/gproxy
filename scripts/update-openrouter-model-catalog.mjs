#!/usr/bin/env node

import { readFile, writeFile } from "node:fs/promises"
import { execFileSync } from "node:child_process"
import path from "node:path"
import { fileURLToPath } from "node:url"

const MODELS_URL = "https://openrouter.ai/api/v1/models?limit=1000&output_modalities=all"
const scriptPath = fileURLToPath(import.meta.url)
const root = path.resolve(path.dirname(scriptPath), "..")
const outputPath = path.join(root, "crates", "gproxy-admin", "assets", "default-model-catalog.json")
const DECIMAL = /^(?:0|[1-9]\d*)(?:\.\d+)?$/
const SUPPORTED_OUTPUT_MODALITIES = new Set(["text", "image", "embeddings", "rerank"])
const TOKEN_UNIT = 1_000_000

export function perMillion(value) {
  if (value == null) return null
  if (typeof value !== "string" || !DECIMAL.test(value)) {
    throw new Error(`invalid decimal price: ${String(value)}`)
  }
  const [integer, fraction = ""] = value.split(".")
  const coefficient = BigInt(`${integer}${fraction}`)
  const scale = fraction.length - 6
  if (coefficient === 0n) return "0"
  if (scale <= 0) return (coefficient * (10n ** BigInt(-scale))).toString()
  const digits = coefficient.toString().padStart(scale + 1, "0")
  const whole = digits.slice(0, -scale)
  const decimal = digits.slice(-scale).replace(/0+$/, "")
  return decimal ? `${whole}.${decimal}` : whole
}

function tokenRate(metric, value, priority, required = false) {
  const price = perMillion(value)
  if (price == null || (!required && price === "0")) return null
  return { metric, unit_size: TOKEN_UNIT, price, priority }
}

function scalarRate(metric, value, priority) {
  if (value == null || value === "0") return null
  if (typeof value !== "string" || !DECIMAL.test(value)) {
    throw new Error(`invalid ${metric} price: ${String(value)}`)
  }
  return { metric, unit_size: 1, price: value, priority }
}

function rates(pricing, author) {
  return [
    tokenRate("input_tokens", pricing.prompt, 0, true),
    tokenRate("output_tokens", pricing.completion, 1, true),
    tokenRate("cached_input_tokens", pricing.input_cache_read, 2),
    tokenRate(
      author === "openai" ? "cache_creation_30m_tokens" : "cache_creation_5m_tokens",
      pricing.input_cache_write,
      3,
    ),
    tokenRate("cache_creation_1h_tokens", pricing.input_cache_write_1h, 4),
    tokenRate("image_output_tokens", pricing.image_output, 5),
    tokenRate("audio_input_tokens", pricing.audio, 6),
    tokenRate("audio_output_tokens", pricing.audio_output, 7),
    tokenRate("cached_audio_input_tokens", pricing.input_audio_cache, 8),
    scalarRate("web_searches", pricing.web_search, 9),
  ].filter(Boolean)
}

function tiers(pricing, author, modelId) {
  if (!Array.isArray(pricing.overrides) || pricing.overrides.length === 0) return null
  return pricing.overrides.map((override) => {
    if (!Number.isInteger(override.min_prompt_tokens) || override.min_prompt_tokens <= 0) {
      throw new Error(`model ${modelId} has an invalid pricing override threshold`)
    }
    const result = { min_prompt_tokens: override.min_prompt_tokens }
    const assign = (field, value) => {
      const price = perMillion(value)
      if (price != null) result[field] = price
    }
    assign("input_price", override.prompt)
    assign("output_price", override.completion)
    assign("cache_read_price", override.input_cache_read)
    assign(
      author === "openai" ? "cache_creation_30m_price" : "cache_creation_5m_price",
      override.input_cache_write,
    )
    assign("cache_creation_1h_price", override.input_cache_write_1h)
    assign("image_output_price", override.image_output)
    return result
  })
}

export function buildCatalog(payload, fetchedAt = new Date().toISOString()) {
  if (!payload || !Array.isArray(payload.data)) {
    throw new Error("OpenRouter response does not contain a data array")
  }
  if (Number.isInteger(payload.total_count) && payload.total_count !== payload.data.length) {
    throw new Error(`OpenRouter response is incomplete: received ${payload.data.length} of ${payload.total_count}`)
  }

  const models = []
  let dynamicPriceModels = 0
  let embeddingModels = 0
  let rerankModels = 0
  for (const model of payload.data) {
    if (typeof model?.id !== "string" || !model.id.includes("/")) {
      throw new Error(`invalid OpenRouter model id: ${String(model?.id)}`)
    }
    const architecture = model.architecture
    const inputModalities = architecture?.input_modalities
    const outputModalities = architecture?.output_modalities
    const supportedParameters = model.supported_parameters
    if (!Array.isArray(inputModalities) || !Array.isArray(outputModalities)
      || !Array.isArray(supportedParameters)) {
      throw new Error(`model ${model.id} has incomplete capabilities`)
    }
    if (model.context_length != null && !Number.isInteger(model.context_length)) {
      throw new Error(`model ${model.id} has an invalid context length`)
    }
    const contextWindow = model.context_length > 0 ? model.context_length : null
    const rawMaxOutputTokens = model.top_provider?.max_completion_tokens
    if (rawMaxOutputTokens != null && !Number.isInteger(rawMaxOutputTokens)) {
      throw new Error(`model ${model.id} has an invalid completion limit`)
    }
    const maxOutputTokens = rawMaxOutputTokens > 0 ? rawMaxOutputTokens : null
    if (outputModalities.includes("embeddings")) embeddingModels += 1
    if (outputModalities.includes("rerank")) rerankModels += 1
    models.push({
      model_id: model.id,
      display_name: typeof model.name === "string" && model.name.trim() ? model.name : null,
      context_window: contextWindow,
      max_output_tokens: maxOutputTokens ?? null,
      input_modalities: [...new Set(inputModalities)].sort(),
      output_modalities: [...new Set(outputModalities)].sort(),
      supported_parameters: [...new Set(supportedParameters)].sort(),
      pricing: modelPricing(model, outputModalities, () => { dynamicPriceModels += 1 }),
    })
  }

  models.sort((left, right) => left.model_id.localeCompare(right.model_id))
  const patterns = new Set()
  for (const model of models) {
    if (model.pricing == null) continue
    if (patterns.has(model.pricing.model_pattern)) {
      throw new Error(`duplicate OpenRouter model basename: ${model.pricing.model_pattern.slice(1, -1)}`)
    }
    patterns.add(model.pricing.model_pattern)
  }
  const pricedModels = models.filter((model) => model.pricing != null)
  return {
    schema_version: 2,
    source: {
      catalog: "openrouter",
      fetched_at: fetchedAt,
      total_models: payload.data.length,
      context_models: models.filter((model) => model.context_window != null).length,
      output_limit_models: models.filter((model) => model.max_output_tokens != null).length,
      priced_models: pricedModels.length,
      dynamic_price_models: dynamicPriceModels,
      embedding_models: embeddingModels,
      rerank_models: rerankModels,
      image_output_priced_models: pricedModels.filter((model) =>
        model.pricing.rates.some((rate) => rate.metric === "image_output_tokens")).length,
    },
    models,
  }
}

export function applyCodexCatalog(catalog, payload, revision = "unknown") {
  if (!payload || !Array.isArray(payload.models)) {
    throw new Error("Codex catalog does not contain a models array")
  }
  const openrouterModels = catalog.models.length
  for (const source of payload.models) {
    if (typeof source?.slug !== "string" || !source.slug.trim()) {
      throw new Error(`invalid Codex model slug: ${String(source?.slug)}`)
    }
    const exact = catalog.models.find((model) => model.model_id === `openai/${source.slug}`)
    const matches = catalog.models.filter((model) =>
      model.model_id.slice(model.model_id.lastIndexOf("/") + 1) === source.slug)
    let target = exact ?? (matches.length === 1 ? matches[0] : null)
    if (!target) {
      target = {
        model_id: `openai/${source.slug}`,
        display_name: null,
        context_window: null,
        max_output_tokens: null,
        input_modalities: [],
        output_modalities: ["text"],
        supported_parameters: [],
        pricing: null,
      }
      catalog.models.push(target)
    }
    const assign = (name, value) => {
      if (value !== undefined && value !== null) target[name] = value
    }
    assign("display_name", source.display_name)
    assign("description", source.description)
    assign("instructions", source.base_instructions ?? source.model_messages?.instructions_template)
    assign("context_window", source.context_window)
    assign("max_context_window", source.max_context_window)
    assign("input_modalities", source.input_modalities)
    assign("supported_reasoning_levels", source.supported_reasoning_levels)
    assign("default_reasoning_level", source.default_reasoning_level)
    assign("service_tiers", source.service_tiers)
    assign("default_service_tier", source.default_service_tier)
    assign("shell_type", source.shell_type)
    assign("support_verbosity", source.support_verbosity)
    assign("default_verbosity", source.default_verbosity)
    assign("supports_reasoning_summary_parameter", source.supports_reasoning_summary_parameter)
    assign("default_reasoning_summary", source.default_reasoning_summary)
    assign("apply_patch_tool_type", source.apply_patch_tool_type)
    assign("web_search_tool_type", source.web_search_tool_type)
    assign("auto_compact_token_limit", source.auto_compact_token_limit)
    assign("effective_context_window_percent", source.effective_context_window_percent)
    assign("supports_image_detail_original", source.supports_image_detail_original)
    assign("supports_search_tool", source.supports_search_tool)
    if (source.truncation_policy) {
      assign("truncation_mode", source.truncation_policy.mode)
      assign("truncation_limit", source.truncation_policy.limit)
    }
    const parameters = new Set(target.supported_parameters ?? [])
    if ((source.supported_reasoning_levels ?? []).length > 0) parameters.add("reasoning_effort")
    if (source.support_verbosity) parameters.add("verbosity")
    target.supported_parameters = [...parameters].sort()
  }
  catalog.models.sort((left, right) => left.model_id.localeCompare(right.model_id))
  catalog.source = {
    ...catalog.source,
    catalog: "openrouter+codex",
    openrouter_models: openrouterModels,
    codex_models: payload.models.length,
    codex_revision: revision,
    total_models: catalog.models.length,
    context_models: catalog.models.filter((model) => model.context_window != null).length,
    output_limit_models: catalog.models.filter((model) => model.max_output_tokens != null).length,
  }
  return catalog
}

function modelPricing(model, outputModalities, dynamic) {
  if (!outputModalities.some((modality) => SUPPORTED_OUTPUT_MODALITIES.has(modality))) return null
  const pricing = model.pricing
  if (!pricing || typeof pricing.prompt !== "string" || typeof pricing.completion !== "string") {
    throw new Error(`model ${model.id} has no prompt/completion pricing`)
  }
  const represented = [
    pricing.prompt,
    pricing.completion,
    pricing.input_cache_read,
    pricing.input_cache_write,
    pricing.input_cache_write_1h,
    pricing.image_output,
    pricing.audio,
    pricing.audio_output,
    pricing.input_audio_cache,
    pricing.web_search,
  ]
  if (represented.some((price) => typeof price === "string" && price.startsWith("-"))) {
    dynamic()
    return null
  }
  if (Array.isArray(pricing.overrides)
    && pricing.overrides.some((override) => !Number.isInteger(override.min_prompt_tokens))) {
    dynamic()
    return null
  }
  const author = model.id.slice(0, model.id.indexOf("/")).replace(/^~/, "")
  const basename = model.id.slice(model.id.lastIndexOf("/") + 1)
  if (basename.includes("*")) throw new Error(`model ${model.id} cannot become a glob pattern`)
  return {
    model_pattern: `*${basename}*`,
    tiers: tiers(pricing, author, model.id),
    priority: 1_000_000 - basename.length,
    rates: rates(pricing, author),
  }
}

async function loadPayload(inputPath) {
  if (inputPath) return JSON.parse(await readFile(path.resolve(process.cwd(), inputPath), "utf8"))
  const headers = { Accept: "application/json", "User-Agent": "gproxy-price-catalog-updater" }
  const apiKey = process.env.OPENROUTER_API_KEY?.trim()
  if (apiKey) headers.Authorization = `Bearer ${apiKey}`
  const response = await fetch(MODELS_URL, { headers })
  if (!response.ok) throw new Error(`OpenRouter models request returned HTTP ${response.status}`)
  return response.json()
}

async function main() {
  const args = process.argv.slice(2)
  let inputPath
  let codexPath = path.join(root, "samples", "codex", "codex-rs", "models-manager", "models.json")
  for (let index = 0; index < args.length; index += 2) {
    const option = args[index]
    const value = args[index + 1]
    if (!value || !["--input", "--codex-input"].includes(option)) {
      throw new Error("usage: node scripts/update-openrouter-model-catalog.mjs [--input response.json] [--codex-input models.json]")
    }
    if (option === "--input") inputPath = value
    if (option === "--codex-input") codexPath = path.resolve(process.cwd(), value)
  }
  const codex = JSON.parse(await readFile(codexPath, "utf8"))
  let revision = "unknown"
  try {
    revision = execFileSync("git", ["-C", path.join(root, "samples", "codex"), "rev-parse", "HEAD"], { encoding: "utf8" }).trim()
  } catch {}
  const catalog = applyCodexCatalog(buildCatalog(await loadPayload(inputPath)), codex, revision)
  await writeFile(outputPath, `${JSON.stringify(catalog)}\n`, "utf8")
  console.log(`Updated ${outputPath} with ${catalog.models.length} models.`)
}

if (process.argv[1] && path.resolve(process.argv[1]) === scriptPath) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error))
    process.exitCode = 1
  })
}

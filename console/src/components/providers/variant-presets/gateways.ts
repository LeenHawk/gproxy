import type { VariantPresetGroup } from "@/components/providers/variant-presets/types"

const openRouter = ["openai", "anthropic", "google-vertex", "google-ai-studio", "amazon-bedrock", "azure", "deepseek", "deepinfra", "together", "fireworks", "groq", "cerebras", "mistral", "xai", "novita"]
const vercel = ["openai", "anthropic", "google", "vertex", "bedrock", "groq", "deepseek", "xai", "mistral", "cohere", "perplexity"]

const source = (key: string, label: string, path: string, values: Array<string>): VariantPresetGroup => ({
  key,
  label,
  entries: values.map((value) => ({
    suffix: `-via-${value}`,
    label: `${path}: ${value}`,
    actions: [{ path, value: [value] }],
  })),
})

export const GATEWAY_SOURCE_BY_CHANNEL: Record<string, VariantPresetGroup> = {
  openrouter: source("openrouter_source", "OpenRouter Provider", "provider.only", openRouter),
  vercel: source("vercel_source", "Vercel Gateway Source", "providerOptions.gateway.only", vercel),
}

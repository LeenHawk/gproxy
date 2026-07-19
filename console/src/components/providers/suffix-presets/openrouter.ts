import type { SuffixGroup } from "./index";

// Common OpenRouter provider slugs (docs: features/provider-routing). The
// custom input in the picker covers endpoint variants like `deepinfra/turbo`.
const VIA = ["openai", "anthropic", "google-vertex", "google-ai-studio", "amazon-bedrock", "azure", "deepseek", "deepinfra", "together", "fireworks", "groq", "cerebras", "mistral", "xai", "novita"] as const;

export const OPENROUTER_PROVIDER_SOURCE_GROUP: SuffixGroup = {
  key: "openrouter_provider_source",
  label: "OpenRouter Provider",
  entries: VIA.map((v) => ({
    suffix: `-via-${v}`,
    label: `provider.only: ${v}`,
    actions: [{ path: "provider.only", value: [v] }],
  })),
};

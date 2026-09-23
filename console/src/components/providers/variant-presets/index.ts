import { CLAUDE_PRESETS, GEMINI_PRESETS } from "@/components/providers/variant-presets/claude-gemini"
import { GATEWAY_SOURCE_BY_CHANNEL } from "@/components/providers/variant-presets/gateways"
import { OPENAI_CHAT_PRESETS, OPENAI_RESPONSES_PRESETS } from "@/components/providers/variant-presets/openai"
import type { VariantPresetGroup, VariantProtocol } from "@/components/providers/variant-presets/types"

export type { VariantAction, VariantPresetEntry, VariantPresetGroup, VariantProtocol } from "@/components/providers/variant-presets/types"

const GROUPS: Record<VariantProtocol, Array<VariantPresetGroup>> = {
  claude: CLAUDE_PRESETS,
  openai_responses: OPENAI_RESPONSES_PRESETS,
  openai_chat: OPENAI_CHAT_PRESETS,
  gemini: GEMINI_PRESETS,
}

export const VARIANT_PROTOCOL_LABELS: Record<VariantProtocol, string> = {
  claude: "Claude Messages",
  openai_responses: "OpenAI Responses",
  openai_chat: "OpenAI Chat Completions",
  gemini: "Gemini GenerateContent",
}

export function variantGroups(protocol: VariantProtocol, channel: string): Array<VariantPresetGroup> {
  const source = GATEWAY_SOURCE_BY_CHANNEL[channel]
  return source ? [...GROUPS[protocol], source] : GROUPS[protocol]
}

export function defaultVariantProtocol(channel: string): VariantProtocol {
  if (["claudeapi", "claudecode", "claudeweb"].includes(channel)) return "claude"
  if (["aistudio", "vertex", "vertexexpress", "geminicli", "antigravity"].includes(channel)) return "gemini"
  return "openai_responses"
}

export function gatewayActionPath(channel: string): string | null {
  if (channel === "openrouter") return "provider.only"
  if (channel === "vercel") return "providerOptions.gateway.only"
  return null
}

export { GATEWAY_SOURCE_BY_CHANNEL }

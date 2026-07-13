import { CLAUDE_GROUPS } from "./claude";
import { OPENAI_RESPONSE_GROUPS, OPENAI_CHAT_GROUPS } from "./openai";
import { GEMINI_GROUPS } from "./gemini";
import { VERCEL_GATEWAY_SOURCE_GROUP } from "./vercel";

export type SuffixProtocol =
  | "claude"
  | "openai_response"
  | "openai_chat_completions"
  | "gemini";

/** One body rewrite the preset injects: set `path` to `value` (action is always "set"). */
export interface SuffixAction {
  path: string;
  value: unknown;
}

export interface SuffixEntry {
  suffix: string;
  label: string;
  actions: SuffixAction[];
}

export interface SuffixGroup {
  /** form key; mutually-exclusive entries within a group. */
  key: string;
  label: string;
  entries: SuffixEntry[];
}

export const SUFFIX_GROUPS_BY_PROTOCOL: Record<SuffixProtocol, SuffixGroup[]> = {
  claude: CLAUDE_GROUPS,
  openai_response: OPENAI_RESPONSE_GROUPS,
  openai_chat_completions: OPENAI_CHAT_GROUPS,
  gemini: GEMINI_GROUPS,
};

export const SUFFIX_PROTOCOL_LABELS: Record<SuffixProtocol, string> = {
  claude: "Claude (Anthropic)",
  openai_response: "OpenAI Responses API",
  openai_chat_completions: "OpenAI Chat Completions",
  gemini: "Gemini",
};

/** Append the Vercel gateway-source group only for the `vercel` channel. */
export function suffixGroupsForChannel(
  protocol: SuffixProtocol,
  channel: string | undefined,
): SuffixGroup[] {
  const base = SUFFIX_GROUPS_BY_PROTOCOL[protocol];
  return channel === "vercel" ? [...base, VERCEL_GATEWAY_SOURCE_GROUP] : base;
}

/** Guess default protocol from channel; falls back to openai_response. */
export function suffixProtocolForChannel(channel: string | undefined): SuffixProtocol {
  switch (channel) {
    case "anthropic":
    case "claudeapi":
    case "claudecode":
    case "claudeweb":
      return "claude";
    case "aistudio":
    case "vertex":
    case "vertexexpress":
    case "geminicli":
    case "antigravity":
      return "gemini";
    default:
      return "openai_response";
  }
}

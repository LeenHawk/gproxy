import type { ChannelCatalogDto, ChannelSettingFieldDto } from "@/api/channels";

export type SecretFamily = ChannelCatalogDto["credential_family"];
export type LoginMode = ChannelCatalogDto["login_modes"][number];
export type EndpointKind = string;
export type ChannelSettingField = ChannelSettingFieldDto;

export const DEFAULT_BASE_URL: Record<string, string> = {
  openai: "https://api.openai.com",
  claudeapi: "https://api.anthropic.com",
  aistudio: "https://generativelanguage.googleapis.com",
  vertexexpress: "https://aiplatform.googleapis.com",
  deepseek: "https://api.deepseek.com",
  groq: "https://api.groq.com/openai",
  nvidia: "https://integrate.api.nvidia.com",
  vercel: "https://ai-gateway.vercel.sh",
  openrouter: "https://openrouter.ai/api",
  opencodezen: "https://opencode.ai/zen/v1",
  opencodego: "https://opencode.ai/zen/go/v1",
  grokbuild: "https://cli-chat-proxy.grok.com/v1",
  claudeweb: "https://claude.ai",
};

export const ENDPOINT_KINDS = [
  "openai_list_models", "claude_list_models", "gemini_list_models",
  "openai_get_model", "claude_get_model", "gemini_get_model",
  "openai_count_tokens", "claude_count_tokens", "gemini_count_tokens",
  "openai_chat_completions", "openai_responses", "openai_realtime", "claude_messages",
  "gemini_generate_content", "gemini_stream_generate_content",
  "openai_embeddings", "gemini_embeddings", "image_generations", "image_edits",
  "openai_compact", "openai_conversations", "usage", "rate_limit_reset",
] as const;

const CUSTOM_ENDPOINTS = ENDPOINT_KINDS.filter(
  (kind) => !["openai_conversations", "usage", "rate_limit_reset"].includes(kind),
);
const ENDPOINTS_BY_CHANNEL: Partial<Record<string, readonly EndpointKind[]>> = {
  openai: ["openai_list_models", "openai_get_model", "openai_chat_completions", "openai_responses", "openai_realtime", "openai_embeddings", "image_generations", "image_edits", "openai_compact"],
  azure: ["openai_list_models", "openai_get_model", "claude_count_tokens", "openai_chat_completions", "openai_responses", "claude_messages", "openai_embeddings", "image_generations", "image_edits", "openai_compact"],
  "aws-bedrock": ["openai_list_models", "openai_get_model", "claude_count_tokens", "claude_messages", "openai_compact"],
  openrouter: ["openai_list_models", "openai_get_model", "openai_chat_completions", "openai_responses", "claude_messages", "openai_embeddings"],
  deepseek: ["openai_list_models", "openai_get_model", "openai_chat_completions", "openai_responses", "claude_messages"],
  groq: ["openai_list_models", "openai_get_model", "openai_chat_completions", "openai_responses"],
  nvidia: ["openai_list_models", "openai_get_model", "openai_chat_completions", "openai_embeddings"],
  opencodezen: ["openai_list_models", "openai_chat_completions", "openai_responses", "claude_messages", "gemini_generate_content", "gemini_stream_generate_content"],
  opencodego: ["openai_list_models", "openai_chat_completions", "openai_responses", "claude_messages"],
  vercel: ["openai_list_models", "openai_get_model", "claude_count_tokens", "openai_chat_completions", "openai_responses", "claude_messages", "openai_embeddings"],
  custom: CUSTOM_ENDPOINTS,
  claudeapi: ["openai_list_models", "claude_list_models", "openai_get_model", "claude_get_model", "claude_count_tokens", "openai_chat_completions", "claude_messages"],
  aistudio: ["openai_list_models", "gemini_list_models", "openai_get_model", "gemini_get_model", "gemini_count_tokens", "openai_chat_completions", "gemini_generate_content", "gemini_stream_generate_content", "gemini_embeddings"],
  vertexexpress: ["gemini_count_tokens", "gemini_generate_content", "gemini_stream_generate_content", "gemini_embeddings"],
  geminicli: ["usage"],
  antigravity: ["usage"],
  claudecode: ["claude_list_models", "claude_get_model", "claude_count_tokens", "claude_messages", "usage"],
  claudeweb: ["usage"],
  codex: ["openai_list_models", "openai_get_model", "openai_responses", "openai_realtime", "openai_compact", "usage", "rate_limit_reset"],
  grokbuild: ["openai_list_models", "openai_get_model", "openai_chat_completions", "openai_responses", "image_generations", "image_edits", "openai_compact"],
  kiro: ["openai_responses"],
};

export interface ChannelMeta {
  id: string;
  displayName: string;
  source: ChannelCatalogDto["source"];
  family: SecretFamily;
  loginModes: LoginMode[];
  settingsFields: readonly ChannelSettingField[];
  usage: boolean;
  endpointKinds: readonly EndpointKind[];
  secretTemplate: unknown;
  hintKey?: string;
  loginParams?: Record<string, unknown>;
}

const API_KEY_IDS = [
  "openai", "azure", "aws-bedrock", "openrouter", "deepseek", "groq", "nvidia",
  "vercel", "custom", "claudeapi", "aistudio", "vertexexpress",
] as const;

// Both OpenCode tiers stay API-key credentials; the console device login is an
// extra way to obtain the key, not a second credential family.
const OPENCODE_SETTINGS: ChannelSettingField[] = [
  {
    key: "console_base_url",
    control: "url",
    label: "OpenCode Console URL",
    required: false,
    placeholder: "https://console.opencode.ai",
  },
];

const OAUTH_TOKENS = { access_token: "", refresh_token: "" };

const DISPLAY_NAMES: Record<string, string> = {
  aistudio: "Google AI Studio", antigravity: "Antigravity", "aws-bedrock": "AWS Bedrock",
  claudeapi: "Claude API", claudecode: "Claude Code", claudeweb: "Claude Web",
  copilotcli: "GitHub Copilot CLI", geminicli: "Gemini CLI",
  grokbuild: "Grok Build", vertexexpress: "Vertex AI Express",
  opencodezen: "OpenCode Zen", opencodego: "OpenCode Go",
};
function builtinMeta(
  id: string,
  family: SecretFamily,
  extra: Partial<ChannelMeta> = {},
): ChannelMeta {
  return {
    id,
    displayName: DISPLAY_NAMES[id] ?? id,
    source: "builtin",
    family,
    loginModes: [],
    settingsFields: [],
    usage: false,
    endpointKinds: ENDPOINTS_BY_CHANNEL[id] ?? [],
    secretTemplate: family === "api_key" ? { api_key: "" } : {},
    ...extra,
  };
}

function oauthMeta(
  id: string,
  loginModes: LoginMode[],
  extra: Partial<ChannelMeta> = {},
): ChannelMeta {
  return builtinMeta(id, "oauth_tokens", {
    loginModes, usage: true, secretTemplate: { ...OAUTH_TOKENS }, ...extra,
  });
}

export const CHANNELS: ChannelMeta[] = [
  ...API_KEY_IDS.map((id) => builtinMeta(id, "api_key", {
    hintKey: id === "aws-bedrock" ? "bedrockApiKeyHint" : undefined,
  })),
  builtinMeta("vertex", "service_account", {
    secretTemplate: { client_email: "", private_key: "", project_id: "" },
  }),
  oauthMeta("geminicli", ["authcode"], {
    secretTemplate: { ...OAUTH_TOKENS, project_id: "" },
    hintKey: "geminiHint",
  }),
  oauthMeta("antigravity", ["authcode"], {
    secretTemplate: { ...OAUTH_TOKENS, project_id: "" },
    hintKey: "geminiHint",
  }),
  oauthMeta("claudecode", ["authcode", "cookie"]),
  oauthMeta("claudeweb", ["cookie"], {
    secretTemplate: { cookie: "", account_uuid: "" },
  }),
  oauthMeta("codex", ["authcode", "device"], {
    secretTemplate: { ...OAUTH_TOKENS, account_id: "" },
  }),
  oauthMeta("grokbuild", ["device"]),
  oauthMeta("kiro", ["authcode", "device"]),
  builtinMeta("copilotcli", "github_token", {
    loginModes: ["device"],
    usage: true,
    secretTemplate: { github_token: "" },
  }),
  builtinMeta("opencodezen", "api_key", {
    loginModes: ["device"],
    settingsFields: OPENCODE_SETTINGS,
  }),
  builtinMeta("opencodego", "api_key", {
    loginModes: ["device"],
    settingsFields: OPENCODE_SETTINGS,
  }),
];

function normalizeRemote(entry: ChannelCatalogDto): ChannelMeta {
  return {
    id: entry.id,
    displayName: entry.display_name,
    source: entry.source,
    family: entry.credential_family,
    loginModes: [...entry.login_modes],
    settingsFields: entry.settings_fields.map((field) => ({ ...field })),
    usage: entry.usage,
    endpointKinds: [...entry.endpoint_kinds],
    secretTemplate: entry.secret_template,
  };
}

export function mergeChannelCatalog(remote: ChannelCatalogDto[]): ChannelMeta[] {
  return remote.map((entry) => {
    const normalized = normalizeRemote(entry);
    const overlay = entry.source === "builtin" ? channelMeta(entry.id) : undefined;
    if (!overlay) return normalized;
    return {
      ...normalized,
      hintKey: overlay.hintKey,
      loginParams: overlay.loginParams,
    };
  });
}

export function channelMeta(
  id: string,
  catalog: readonly ChannelMeta[] = CHANNELS,
): ChannelMeta | undefined {
  return catalog.find((channel) => channel.id === id);
}

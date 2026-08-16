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
  kimiapi: "https://api.moonshot.cn",
  kimicode: "https://api.kimi.com/coding/v1",
  nvidia: "https://integrate.api.nvidia.com",
  xai: "https://api.x.ai",
  vercel: "https://ai-gateway.vercel.sh",
  openrouter: "https://openrouter.ai/api",
  "cloudflare-ai-gateway": "https://api.cloudflare.com",
  dashscope: "https://dashscope.aliyuncs.com",
  cline: "https://api.cline.bot/api/v1",
  opencodezen: "https://opencode.ai/zen/v1",
  opencodego: "https://opencode.ai/zen/go/v1",
  grokbuild: "https://cli-chat-proxy.grok.com/v1",
  workbuddy: "https://copilot.tencent.com",
  claudeweb: "https://claude.ai",
};

export const ENDPOINT_KINDS = [
  "openai_list_models", "claude_list_models", "gemini_list_models",
  "openai_get_model", "claude_get_model", "gemini_get_model",
  "openai_count_tokens", "claude_count_tokens", "gemini_count_tokens",
  "openai_chat_completions", "openai_responses", "openai_realtime", "claude_messages",
  "gemini_generate_content", "gemini_stream_generate_content",
  "openai_embeddings", "gemini_embeddings", "openai_rerank",
  "openai_audio_speech", "openai_audio_transcriptions", "openai_audio_translations",
  "image_generations", "image_edits",
  "openai_video_create", "openai_video_retrieve", "openai_video_list", "openai_video_delete",
  "openai_video_content", "openai_video_remix", "openai_video_character_create",
  "openai_video_character_get", "openai_video_edit", "openai_video_extend",
  "openai_compact", "openai_conversations", "usage", "rate_limit_reset",
] as const;

const CUSTOM_ENDPOINTS = ENDPOINT_KINDS.filter(
  (kind) => !["openai_conversations", "usage", "rate_limit_reset"].includes(kind),
);
const ENDPOINTS_BY_CHANNEL: Partial<Record<string, readonly EndpointKind[]>> = {
  openai: ["openai_list_models", "openai_get_model", "openai_chat_completions", "openai_responses", "openai_realtime", "openai_embeddings", "openai_audio_speech", "openai_audio_transcriptions", "openai_audio_translations", "image_generations", "image_edits", "openai_video_create", "openai_video_retrieve", "openai_video_list", "openai_video_delete", "openai_video_content", "openai_video_remix", "openai_video_character_create", "openai_video_character_get", "openai_video_edit", "openai_video_extend", "openai_compact"],
  azure: ["openai_list_models", "openai_get_model", "claude_count_tokens", "openai_chat_completions", "openai_responses", "claude_messages", "openai_embeddings", "image_generations", "image_edits", "openai_video_create", "openai_video_retrieve", "openai_video_list", "openai_video_delete", "openai_video_content", "openai_video_remix", "openai_video_character_create", "openai_video_character_get", "openai_video_edit", "openai_video_extend", "openai_compact"],
  "aws-bedrock": ["openai_list_models", "openai_get_model", "claude_count_tokens", "claude_messages", "openai_compact", "openai_video_create", "openai_video_retrieve"],
  openrouter: ["openai_list_models", "openai_get_model", "openai_chat_completions", "openai_responses", "claude_messages", "openai_embeddings", "openai_audio_speech", "openai_audio_transcriptions", "openai_rerank", "image_generations", "image_edits", "openai_video_create", "openai_video_retrieve", "openai_video_content"],
  "cloudflare-ai-gateway": ["openai_chat_completions", "openai_responses", "claude_messages"],
  dashscope: ["openai_list_models", "openai_get_model", "openai_chat_completions", "openai_responses", "claude_messages", "openai_embeddings", "openai_rerank", "image_generations", "image_edits", "openai_compact"],
  deepseek: ["openai_list_models", "openai_get_model", "openai_chat_completions", "openai_responses", "claude_messages"],
  groq: ["openai_list_models", "openai_get_model", "openai_chat_completions", "openai_responses"],
  kimiapi: ["openai_list_models", "openai_get_model", "openai_chat_completions", "openai_responses", "openai_embeddings", "image_generations"],
  kimicode: ["openai_list_models", "claude_count_tokens", "openai_chat_completions", "openai_responses", "claude_messages", "gemini_generate_content", "gemini_stream_generate_content", "openai_embeddings", "openai_compact", "usage"],
  nvidia: ["openai_list_models", "openai_get_model", "openai_chat_completions", "openai_embeddings"],
  xai: ["openai_list_models", "openai_get_model", "openai_chat_completions", "openai_responses", "openai_audio_speech", "openai_audio_transcriptions", "image_generations", "image_edits", "openai_video_create", "openai_video_retrieve", "openai_video_edit", "openai_video_extend", "openai_compact"],
  opencodezen: ["openai_list_models", "openai_chat_completions", "openai_responses", "claude_messages", "gemini_generate_content", "gemini_stream_generate_content"],
  opencodego: ["openai_list_models", "openai_chat_completions", "openai_responses", "claude_messages"],
  cline: ["openai_list_models", "openai_chat_completions", "usage"],
  vercel: ["openai_list_models", "openai_get_model", "claude_count_tokens", "openai_chat_completions", "openai_responses", "claude_messages", "openai_embeddings"],
  custom: CUSTOM_ENDPOINTS,
  claudeapi: ["openai_list_models", "claude_list_models", "openai_get_model", "claude_get_model", "claude_count_tokens", "openai_chat_completions", "claude_messages"],
  aistudio: ["openai_list_models", "gemini_list_models", "openai_get_model", "gemini_get_model", "gemini_count_tokens", "openai_chat_completions", "gemini_generate_content", "gemini_stream_generate_content", "gemini_embeddings", "openai_video_create", "openai_video_retrieve"],
  vertex: ["openai_video_create", "openai_video_retrieve"],
  vertexexpress: ["gemini_count_tokens", "gemini_generate_content", "gemini_stream_generate_content", "gemini_embeddings"],
  geminicli: ["usage"],
  antigravity: ["usage"],
  claudecode: ["claude_list_models", "claude_get_model", "claude_count_tokens", "claude_messages", "usage"],
  claudeweb: ["usage"],
  codex: ["openai_list_models", "openai_get_model", "openai_responses", "openai_realtime", "image_generations", "image_edits", "openai_compact", "usage", "rate_limit_reset"],
  grokbuild: ["openai_list_models", "openai_get_model", "openai_chat_completions", "openai_responses", "openai_audio_speech", "openai_audio_transcriptions", "image_generations", "image_edits", "openai_video_create", "openai_video_retrieve", "openai_video_edit", "openai_video_extend", "openai_compact"],
  workbuddy: ["openai_list_models", "openai_chat_completions", "openai_responses", "claude_messages", "gemini_generate_content", "gemini_stream_generate_content", "image_generations", "image_edits", "usage"],
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
  "openai", "azure", "aws-bedrock", "openrouter", "cloudflare-ai-gateway", "dashscope", "deepseek", "groq", "kimiapi", "nvidia", "xai",
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

const AWS_BEDROCK_SETTINGS: ChannelSettingField[] = [{
  key: "video_output_s3_uri",
  control: "text",
  label: "Video output S3 URI",
  required: false,
  placeholder: "s3://bucket/prefix",
}];

const GROKBUILD_SETTINGS: ChannelSettingField[] = [{
  key: "xai_api_base_url",
  control: "url",
  label: "xAI media API URL",
  required: false,
  default: "https://api.x.ai/v1",
  placeholder: "https://api.x.ai/v1",
}];

const KIMICODE_SETTINGS: ChannelSettingField[] = [{
  key: "oauth_host",
  control: "url",
  label: "Kimi OAuth URL",
  required: false,
  default: "https://auth.kimi.com",
  placeholder: "https://auth.kimi.com",
}];

const OAUTH_TOKENS = { access_token: "", refresh_token: "" };

const DISPLAY_NAMES: Record<string, string> = {
  aistudio: "Google AI Studio", antigravity: "Antigravity", "aws-bedrock": "AWS Bedrock",
  azure: "Microsoft Azure", claudeapi: "Claude API", claudecode: "Claude Code",
  claudeweb: "Claude Web", cline: "Cline",
  "cloudflare-ai-gateway": "Cloudflare AI Gateway",
  dashscope: "Alibaba Qwen",
  codex: "OpenAI Codex", copilotcli: "GitHub Copilot CLI", custom: "Custom", deepseek: "DeepSeek",
  geminicli: "Gemini CLI", groq: "Groq", grokbuild: "Grok Build", kiro: "Kiro", kimiapi: "Kimi API", kimicode: "Kimi Code",
  nvidia: "NVIDIA", openai: "OpenAI", opencodezen: "OpenCode Zen",
  opencodego: "OpenCode Go", openrouter: "OpenRouter", vercel: "Vercel AI Gateway",
  vertex: "Google Vertex AI", vertexexpress: "Vertex AI Express",
  workbuddy: "WorkBuddy",
  xai: "xAI",
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
    settingsFields: id === "aws-bedrock" ? AWS_BEDROCK_SETTINGS : [],
    ...(id === "cloudflare-ai-gateway"
      ? { secretTemplate: { api_key: "", account_id: "", gateway_id: "default" } }
      : {}),
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
  oauthMeta("grokbuild", ["device"], { settingsFields: GROKBUILD_SETTINGS }),
  oauthMeta("kimicode", ["device"], {
    settingsFields: KIMICODE_SETTINGS,
    secretTemplate: { ...OAUTH_TOKENS, device_id: "" },
  }),
  oauthMeta("workbuddy", ["device"], {
    secretTemplate: {
      ...OAUTH_TOKENS,
      user_id: "",
      enterprise_id: "",
      department_full_name: "",
      domain: "",
    },
  }),
  builtinMeta("cline", "api_key", {
    loginModes: ["device"],
    usage: true,
  }),
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

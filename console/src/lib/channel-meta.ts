export type SecretFamily = "api_key" | "oauth_tokens" | "service_account" | "github_token";

/** Default base_url per channel. Absent = no public configurable default. */
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
  grokbuild: "https://api.x.ai/v1",
  chatgpt: "https://chatgpt.com",
  claudeweb: "https://claude.ai",
  tasklet: "https://api.tasklet.ai",
};

export const ENDPOINT_KINDS = [
  "openai_list_models", "claude_list_models", "gemini_list_models",
  "openai_get_model", "claude_get_model", "gemini_get_model",
  "openai_count_tokens", "claude_count_tokens", "gemini_count_tokens",
  "openai_chat_completions", "openai_responses", "claude_messages",
  "gemini_generate_content", "gemini_stream_generate_content",
  "openai_embeddings", "gemini_embeddings", "image_generations", "image_edits",
  "openai_compact", "openai_conversations", "usage", "rate_limit_reset",
] as const;
export type EndpointKind = (typeof ENDPOINT_KINDS)[number];

const CUSTOM_ENDPOINTS = ENDPOINT_KINDS.filter(
  (kind) => !["openai_conversations", "usage", "rate_limit_reset"].includes(kind),
);
const ENDPOINTS_BY_CHANNEL: Partial<Record<string, readonly EndpointKind[]>> = {
  openai: ["openai_list_models", "openai_get_model", "openai_chat_completions", "openai_responses", "openai_embeddings", "image_generations", "image_edits", "openai_compact"],
  openrouter: ["openai_list_models", "openai_get_model", "openai_chat_completions", "openai_responses", "claude_messages", "openai_embeddings"],
  deepseek: ["openai_list_models", "openai_get_model", "openai_chat_completions", "claude_messages"],
  groq: ["openai_list_models", "openai_get_model", "openai_chat_completions", "openai_responses"],
  nvidia: ["openai_list_models", "openai_get_model", "openai_chat_completions", "openai_embeddings"],
  vercel: ["openai_list_models", "openai_get_model", "claude_count_tokens", "openai_chat_completions", "openai_responses", "claude_messages", "openai_embeddings"],
  custom: CUSTOM_ENDPOINTS,
  claudeapi: ["openai_list_models", "claude_list_models", "openai_get_model", "claude_get_model", "claude_count_tokens", "openai_chat_completions", "claude_messages"],
  aistudio: ["openai_list_models", "gemini_list_models", "openai_get_model", "gemini_get_model", "gemini_count_tokens", "openai_chat_completions", "gemini_generate_content", "gemini_stream_generate_content", "gemini_embeddings"],
  vertexexpress: ["gemini_count_tokens", "gemini_generate_content", "gemini_stream_generate_content", "gemini_embeddings"],
  geminicli: ["usage"],
  antigravity: ["usage"],
  claudecode: ["claude_list_models", "claude_get_model", "claude_count_tokens", "claude_messages", "usage"],
  claudeweb: ["usage"],
  codex: ["openai_list_models", "openai_get_model", "openai_responses", "openai_compact", "usage", "rate_limit_reset"],
  grokbuild: ["openai_list_models", "openai_get_model", "openai_chat_completions", "openai_responses", "image_generations", "image_edits", "openai_compact"],
  kiro: ["openai_responses"],
};
export type LoginMode = "authcode" | "device" | "cookie";

export interface ChannelMeta {
  id: string;
  family: SecretFamily;
  loginModes: LoginMode[];
  /** GET /admin/credentials/{id}/usage supported */
  usage: boolean;
  /** Exact upstream URLs that this channel resolves from settings_json.endpoints. */
  endpointKinds: readonly EndpointKind[];
  /** Prefill for the manual secret editor */
  secretTemplate: Record<string, unknown>;
  /** providers:secret.* extra hint key, if any */
  hintKey?: string;
  /** Extra params posted to authcode_start. geminicli needs `code_only:false` so
   *  it uses the loopback redirect (a pasteable `?code=&state=` callback URL)
   *  instead of the headless codeassist page that only shows a bare code. */
  loginParams?: Record<string, unknown>;
}

const API_KEY_IDS = [
  "openai", "openrouter", "deepseek", "groq", "nvidia",
  "vercel", "custom", "claudeapi", "aistudio", "vertexexpress",
] as const;

const OAUTH_TOKENS = { access_token: "", refresh_token: "" };

export const CHANNELS: ChannelMeta[] = [
  ...API_KEY_IDS.map((id) => ({
    id: id as string,
    family: "api_key" as const,
    loginModes: [] as LoginMode[],
    usage: false,
    endpointKinds: ENDPOINTS_BY_CHANNEL[id] ?? [],
    secretTemplate: { api_key: "" },
  })),
  {
    id: "vertex",
    family: "service_account",
    loginModes: [],
    usage: false,
    endpointKinds: [],
    secretTemplate: { client_email: "", private_key: "", project_id: "" },
  },
  {
    id: "geminicli",
    family: "oauth_tokens",
    loginModes: ["authcode"],
    usage: true,
    endpointKinds: ENDPOINTS_BY_CHANNEL.geminicli ?? [],
    secretTemplate: { ...OAUTH_TOKENS, project_id: "" },
    hintKey: "geminiHint",
  },
  {
    id: "antigravity",
    family: "oauth_tokens",
    loginModes: ["authcode"],
    usage: true,
    endpointKinds: ENDPOINTS_BY_CHANNEL.antigravity ?? [],
    secretTemplate: { ...OAUTH_TOKENS, project_id: "" },
    hintKey: "geminiHint",
  },
  {
    id: "claudecode",
    family: "oauth_tokens",
    loginModes: ["authcode", "cookie"],
    usage: true,
    endpointKinds: ENDPOINTS_BY_CHANNEL.claudecode ?? [],
    secretTemplate: { ...OAUTH_TOKENS },
  },
  {
    // Claude consumer web backend (native-only `channel-claudeweb` feature).
    // Cookie login validates sessionKey and stores the selected chat org UUID.
    id: "claudeweb",
    family: "oauth_tokens",
    loginModes: ["cookie"],
    usage: true,
    endpointKinds: ENDPOINTS_BY_CHANNEL.claudeweb ?? [],
    secretTemplate: { cookie: "", account_uuid: "" },
  },
  {
    id: "codex",
    family: "oauth_tokens",
    loginModes: ["authcode", "device"],
    usage: true,
    endpointKinds: ENDPOINTS_BY_CHANNEL.codex ?? [],
    secretTemplate: { ...OAUTH_TOKENS, account_id: "" },
  },
  {
    id: "grokbuild",
    family: "oauth_tokens",
    loginModes: ["device"],
    usage: true,
    endpointKinds: ENDPOINTS_BY_CHANNEL.grokbuild ?? [],
    secretTemplate: { ...OAUTH_TOKENS },
  },
  {
    id: "kiro",
    family: "oauth_tokens",
    loginModes: ["authcode", "device"],
    usage: true,
    endpointKinds: ENDPOINTS_BY_CHANNEL.kiro ?? [],
    secretTemplate: { ...OAUTH_TOKENS },
  },
  {
    id: "copilotcli",
    family: "github_token",
    loginModes: ["device"],
    usage: true,
    endpointKinds: [],
    secretTemplate: { github_token: "" },
  },
  {
    // ChatGPT consumer web backend (native-only `channel-chatgpt` feature).
    // Operator pastes a chatgpt.com session cookie; cookie_exchange mints the
    // access_token + warms the sentinel / __cf_bm anti-bot state into the secret.
    id: "chatgpt",
    family: "oauth_tokens",
    loginModes: ["cookie"],
    usage: false,
    endpointKinds: [],
    secretTemplate: { access_token: "", cookie: "" },
  },
  {
    // Tasklet Agent API (native-only `channel-tasklet` feature). The browser
    // session token and workspace id are entered manually from a signed-in SPA.
    id: "tasklet",
    family: "oauth_tokens",
    loginModes: [],
    usage: false,
    endpointKinds: [],
    secretTemplate: { session_token: "", workspace_id: "" },
  },
];

export function channelMeta(id: string): ChannelMeta | undefined {
  return CHANNELS.find((c) => c.id === id);
}

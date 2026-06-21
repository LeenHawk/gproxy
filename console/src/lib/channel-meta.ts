export type SecretFamily = "api_key" | "oauth_tokens" | "service_account" | "github_token";

/** Default base_url per channel. Absent = channel has no public default (custom requires explicit input). */
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
};
export type LoginMode = "authcode" | "device" | "cookie";

export interface ChannelMeta {
  id: string;
  family: SecretFamily;
  loginModes: LoginMode[];
  /** GET /admin/credentials/{id}/usage supported */
  usage: boolean;
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
    secretTemplate: { api_key: "" },
  })),
  {
    id: "vertex",
    family: "service_account",
    loginModes: [],
    usage: false,
    secretTemplate: { client_email: "", private_key: "", project_id: "" },
  },
  {
    id: "geminicli",
    family: "oauth_tokens",
    loginModes: ["authcode"],
    usage: true,
    secretTemplate: { ...OAUTH_TOKENS, project_id: "" },
    hintKey: "geminiHint",
  },
  {
    id: "antigravity",
    family: "oauth_tokens",
    loginModes: ["authcode"],
    usage: true,
    secretTemplate: { ...OAUTH_TOKENS, project_id: "" },
    hintKey: "geminiHint",
  },
  {
    id: "claudecode",
    family: "oauth_tokens",
    loginModes: ["authcode", "cookie"],
    usage: true,
    secretTemplate: { ...OAUTH_TOKENS },
  },
  {
    id: "codex",
    family: "oauth_tokens",
    loginModes: ["authcode", "device"],
    usage: true,
    secretTemplate: { ...OAUTH_TOKENS, account_id: "" },
  },
  {
    id: "kiro",
    family: "oauth_tokens",
    loginModes: ["authcode", "device"],
    usage: true,
    secretTemplate: { ...OAUTH_TOKENS },
  },
  {
    id: "copilotcli",
    family: "github_token",
    loginModes: ["device"],
    usage: true,
    secretTemplate: { github_token: "" },
  },
];

export function channelMeta(id: string): ChannelMeta | undefined {
  return CHANNELS.find((c) => c.id === id);
}

type FieldType = "text" | "boolean" | "integer" | "textarea" | "json";

export type ChannelField = {
  key: string;
  label: string;
  type: FieldType;
  optional?: boolean;
};

type ChannelSettingsConfig = {
  defaults: Record<string, string>;
  fields: ChannelField[];
};

type ChannelCredentialConfig = {
  fields: ChannelField[];
};

export const ALL_CHANNEL_IDS = [
  "custom",
  "openai",
  "anthropic",
  "aistudio",
  "vertex",
  "vertexexpress",
  "geminicli",
  "antigravity",
  "claudecode",
  "codex",
  "nvidia",
  "deepseek",
  "groq",
  "openrouter",
] as const;

/// Common settings fields appended to every channel so sanitize_rules
/// is always configurable regardless of channel type.
const COMMON_SETTINGS_FIELDS: ChannelField[] = [
  { key: "sanitize_rules", label: "sanitize_rules", type: "json", optional: true },
];

export const SETTINGS_CHANNEL_CONFIG: Record<string, ChannelSettingsConfig> = {
  openai: {
    defaults: { base_url: "https://api.openai.com", user_agent: "" },
    fields: [
      { key: "base_url", label: "base_url", type: "text" },
      { key: "user_agent", label: "user_agent", type: "text", optional: true },
    ],
  },
  anthropic: {
    defaults: { base_url: "https://api.anthropic.com", user_agent: "" },
    fields: [
      { key: "base_url", label: "base_url", type: "text" },
      { key: "user_agent", label: "user_agent", type: "text", optional: true },
      { key: "enable_magic_cache", label: "enable_magic_cache", type: "boolean", optional: true },
      { key: "cache_breakpoints", label: "cache_breakpoints", type: "json", optional: true },
      { key: "extra_beta_headers", label: "extra_beta_headers", type: "json", optional: true },
    ],
  },
  aistudio: {
    defaults: { base_url: "https://generativelanguage.googleapis.com", user_agent: "" },
    fields: [
      { key: "base_url", label: "base_url", type: "text" },
      { key: "user_agent", label: "user_agent", type: "text", optional: true },
    ],
  },
  vertex: {
    defaults: {
      base_url: "https://aiplatform.googleapis.com",
      user_agent: "",
      oauth_token_url: "https://oauth2.googleapis.com/token",
    },
    fields: [
      { key: "base_url", label: "base_url", type: "text" },
      { key: "user_agent", label: "user_agent", type: "text", optional: true },
      { key: "oauth_token_url", label: "oauth_token_url", type: "text", optional: true },
    ],
  },
  vertexexpress: {
    defaults: { base_url: "https://aiplatform.googleapis.com", user_agent: "" },
    fields: [
      { key: "base_url", label: "base_url", type: "text" },
      { key: "user_agent", label: "user_agent", type: "text", optional: true },
    ],
  },
  geminicli: {
    defaults: {
      base_url: "https://cloudcode-pa.googleapis.com",
      user_agent: "",
      oauth_authorize_url: "https://accounts.google.com/o/oauth2/v2/auth",
      oauth_token_url: "https://oauth2.googleapis.com/token",
      oauth_userinfo_url: "https://www.googleapis.com/oauth2/v2/userinfo",
    },
    fields: [
      { key: "base_url", label: "base_url", type: "text" },
      { key: "user_agent", label: "user_agent", type: "text", optional: true },
      { key: "oauth_authorize_url", label: "oauth_authorize_url", type: "text" },
      { key: "oauth_token_url", label: "oauth_token_url", type: "text" },
      { key: "oauth_userinfo_url", label: "oauth_userinfo_url", type: "text" },
    ],
  },
  antigravity: {
    defaults: {
      base_url: "https://daily-cloudcode-pa.sandbox.googleapis.com",
      user_agent: "antigravity/1.15.8 (Windows; AMD64)",
      oauth_authorize_url: "https://accounts.google.com/o/oauth2/v2/auth",
      oauth_token_url: "https://oauth2.googleapis.com/token",
      oauth_userinfo_url: "https://www.googleapis.com/oauth2/v1/userinfo?alt=json",
    },
    fields: [
      { key: "base_url", label: "base_url", type: "text" },
      { key: "user_agent", label: "user_agent", type: "text", optional: true },
      { key: "oauth_authorize_url", label: "oauth_authorize_url", type: "text" },
      { key: "oauth_token_url", label: "oauth_token_url", type: "text" },
      { key: "oauth_userinfo_url", label: "oauth_userinfo_url", type: "text" },
    ],
  },
  claudecode: {
    defaults: {
      base_url: "https://api.anthropic.com",
      user_agent: "claude-code/2.1.89",
      claude_ai_base_url: "https://claude.ai",
      platform_base_url: "https://platform.claude.com",
    },
    fields: [
      { key: "base_url", label: "base_url", type: "text" },
      { key: "user_agent", label: "user_agent", type: "text", optional: true },
      { key: "claude_ai_base_url", label: "claude_ai_base_url", type: "text" },
      { key: "platform_base_url", label: "platform_base_url", type: "text" },
      { key: "enable_magic_cache", label: "enable_magic_cache", type: "boolean", optional: true },
      { key: "cache_breakpoints", label: "cache_breakpoints", type: "json", optional: true },
      { key: "prelude_text", label: "prelude_text", type: "textarea", optional: true },
      { key: "extra_beta_headers", label: "extra_beta_headers", type: "json", optional: true },
    ],
  },
  codex: {
    defaults: {
      base_url: "https://chatgpt.com/backend-api/codex",
      user_agent: "codex_vscode/0.99.0",
      oauth_issuer_url: "https://auth.openai.com",
    },
    fields: [
      { key: "base_url", label: "base_url", type: "text" },
      { key: "user_agent", label: "user_agent", type: "text", optional: true },
      { key: "oauth_issuer_url", label: "oauth_issuer_url", type: "text", optional: true },
    ],
  },
  nvidia: {
    defaults: { base_url: "https://integrate.api.nvidia.com", user_agent: "" },
    fields: [
      { key: "base_url", label: "base_url", type: "text" },
      { key: "user_agent", label: "user_agent", type: "text", optional: true },
    ],
  },
  deepseek: {
    defaults: { base_url: "https://api.deepseek.com", user_agent: "" },
    fields: [
      { key: "base_url", label: "base_url", type: "text" },
      { key: "user_agent", label: "user_agent", type: "text", optional: true },
    ],
  },
  groq: {
    defaults: { base_url: "https://api.groq.com/openai", user_agent: "" },
    fields: [
      { key: "base_url", label: "base_url", type: "text" },
      { key: "user_agent", label: "user_agent", type: "text", optional: true },
    ],
  },
  openrouter: {
    defaults: { base_url: "https://openrouter.ai/api/v1", user_agent: "" },
    fields: [
      { key: "base_url", label: "base_url", type: "text" },
      { key: "user_agent", label: "user_agent", type: "text", optional: true },
    ],
  },
  custom: {
    defaults: { base_url: "", user_agent: "" },
    fields: [
      { key: "base_url", label: "base_url", type: "text" },
      { key: "user_agent", label: "user_agent", type: "text", optional: true },
      { key: "mask_table", label: "mask_table", type: "json", optional: true },
    ],
  },
};

export const CREDENTIAL_CHANNEL_CONFIG: Record<string, ChannelCredentialConfig> = {
  openai: { fields: [{ key: "api_key", label: "api_key", type: "textarea" }] },
  anthropic: { fields: [{ key: "api_key", label: "api_key", type: "textarea" }] },
  aistudio: { fields: [{ key: "api_key", label: "api_key", type: "textarea" }] },
  vertex: {
    fields: [
      { key: "client_email", label: "client_email", type: "text" },
      { key: "private_key", label: "private_key", type: "textarea" },
      { key: "project_id", label: "project_id", type: "text" },
    ],
  },
  vertexexpress: { fields: [{ key: "access_token", label: "access_token", type: "textarea" }] },
  geminicli: {
    fields: [
      { key: "access_token", label: "access_token", type: "textarea" },
      { key: "refresh_token", label: "refresh_token", type: "textarea" },
      { key: "expires_at_ms", label: "expires_at_ms", type: "integer" },
      { key: "project_id", label: "project_id", type: "text" },
      { key: "client_id", label: "client_id", type: "text", optional: true },
      { key: "client_secret", label: "client_secret", type: "text", optional: true },
      { key: "user_email", label: "user_email", type: "text", optional: true },
    ],
  },
  antigravity: {
    fields: [
      { key: "access_token", label: "access_token", type: "textarea" },
      { key: "refresh_token", label: "refresh_token", type: "textarea" },
      { key: "expires_at_ms", label: "expires_at_ms", type: "integer" },
      { key: "project_id", label: "project_id", type: "text" },
      { key: "client_id", label: "client_id", type: "text", optional: true },
      { key: "client_secret", label: "client_secret", type: "text", optional: true },
      { key: "user_email", label: "user_email", type: "text", optional: true },
    ],
  },
  claudecode: {
    fields: [
      { key: "access_token", label: "access_token", type: "textarea" },
      { key: "refresh_token", label: "refresh_token", type: "textarea", optional: true },
      { key: "expires_at_ms", label: "expires_at_ms", type: "integer" },
      { key: "device_id", label: "device_id", type: "text", optional: true },
      { key: "account_uuid", label: "account_uuid", type: "text", optional: true },
      { key: "subscription_type", label: "subscription_type", type: "text", optional: true },
      { key: "rate_limit_tier", label: "rate_limit_tier", type: "text", optional: true },
      { key: "cookie", label: "cookie", type: "textarea", optional: true },
      { key: "user_email", label: "user_email", type: "text", optional: true },
    ],
  },
  codex: {
    fields: [
      { key: "access_token", label: "access_token", type: "textarea" },
      { key: "refresh_token", label: "refresh_token", type: "textarea", optional: true },
      { key: "id_token", label: "id_token", type: "textarea", optional: true },
      { key: "user_email", label: "user_email", type: "text", optional: true },
      { key: "account_id", label: "account_id", type: "text", optional: true },
      { key: "expires_at_ms", label: "expires_at_ms", type: "integer" },
    ],
  },
  nvidia: { fields: [{ key: "api_key", label: "api_key", type: "textarea" }] },
  deepseek: { fields: [{ key: "api_key", label: "api_key", type: "textarea" }] },
  groq: { fields: [{ key: "api_key", label: "api_key", type: "textarea" }] },
  openrouter: { fields: [{ key: "api_key", label: "api_key", type: "textarea" }] },
  custom: { fields: [{ key: "api_key", label: "api_key", type: "textarea" }] },
};

export function settingsFieldsForChannel(channel: string): ChannelField[] {
  const channelFields =
    SETTINGS_CHANNEL_CONFIG[channel]?.fields ?? SETTINGS_CHANNEL_CONFIG.custom.fields;
  return [...channelFields, ...COMMON_SETTINGS_FIELDS];
}

export function credentialFieldsForChannel(channel: string): ChannelField[] {
  return CREDENTIAL_CHANNEL_CONFIG[channel]?.fields ?? CREDENTIAL_CHANNEL_CONFIG.custom.fields;
}

export function defaultSettingsForChannel(channel: string): Record<string, string> {
  return { ...(SETTINGS_CHANNEL_CONFIG[channel]?.defaults ?? SETTINGS_CHANNEL_CONFIG.custom.defaults) };
}

export function emptyCredentialValuesForChannel(channel: string): Record<string, string> {
  return Object.fromEntries(credentialFieldsForChannel(channel).map((field) => [field.key, ""]));
}

export function settingsValuesFromJson(
  channel: string,
  value: Record<string, unknown>,
): Record<string, string> {
  const current = defaultSettingsForChannel(channel);
  for (const field of settingsFieldsForChannel(channel)) {
    const raw = value[field.key];
    if (raw === undefined || raw === null) {
      continue;
    }
    if (field.type === "json") {
      current[field.key] = typeof raw === "string" ? raw : JSON.stringify(raw, null, 2);
    } else {
      current[field.key] = typeof raw === "string" ? raw : JSON.stringify(raw);
    }
  }
  return current;
}

export function credentialValuesFromJson(
  channel: string,
  value: Record<string, unknown>,
): Record<string, string> {
  const current = emptyCredentialValuesForChannel(channel);
  for (const field of credentialFieldsForChannel(channel)) {
    const raw = value[field.key];
    if (raw === undefined || raw === null) {
      continue;
    }
    if (field.type === "json") {
      current[field.key] = typeof raw === "string" ? raw : JSON.stringify(raw, null, 2);
    } else {
      current[field.key] = typeof raw === "string" ? raw : JSON.stringify(raw);
    }
  }
  return current;
}

export function buildChannelSettingsJson(
  channel: string,
  values: Record<string, string>,
): Record<string, unknown> {
  return buildObjectFromFields(settingsFieldsForChannel(channel), values);
}

export function buildCredentialJson(
  channel: string,
  values: Record<string, string>,
): Record<string, unknown> {
  return buildObjectFromFields(credentialFieldsForChannel(channel), values);
}

function buildObjectFromFields(
  fields: ChannelField[],
  values: Record<string, string>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const field of fields) {
    const raw = values[field.key] ?? "";
    const trimmed = raw.trim();
    if (field.optional && trimmed === "") {
      continue;
    }
    if (field.type === "boolean") {
      result[field.key] = raw === "true";
      continue;
    }
    if (field.type === "integer") {
      result[field.key] = trimmed === "" ? 0 : Number.parseInt(trimmed, 10);
      continue;
    }
    if (field.type === "json") {
      if (trimmed === "") {
        continue;
      }
      try {
        result[field.key] = JSON.parse(trimmed);
      } catch {
        result[field.key] = trimmed;
      }
      continue;
    }
    result[field.key] = raw;
  }
  return result;
}

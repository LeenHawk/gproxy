type FieldType = "text" | "boolean" | "integer" | "textarea" | "json" | "select";

export type ChannelField = {
  key: string;
  label: string;
  type: FieldType;
  optional?: boolean;
  options?: { value: string; label: string }[];
};

type ChannelSettingsConfig = {
  defaults: Record<string, string>;
  fields: ChannelField[];
};

type ChannelCredentialConfig = {
  fields: ChannelField[];
};

const DEFAULT_CLAUDECODE_FINGERPRINT = JSON.stringify(
  {
    cli_version: "2.1.112",
    user_type: "external",
    entrypoint: "cli",
    stainless_lang: "js",
    stainless_package_version: "0.81.0",
    stainless_runtime: "node",
    stainless_runtime_version: "v22.20.0",
    stainless_os: "Linux",
    stainless_arch: "x64",
    stainless_timeout: "600",
  },
  null,
  2,
);

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
  "chatgpt",
  "nvidia",
  "deepseek",
  "groq",
  "openrouter",
  "vercel",
] as const;

/// Common settings fields appended to every channel so sanitize_rules
/// is always configurable regardless of channel type.
const COMMON_SETTINGS_FIELDS: ChannelField[] = [
  { key: "max_retries_on_429", label: "max_retries_on_429", type: "integer", optional: true },
  {
    key: "rotation_strategy",
    label: "rotation_strategy",
    type: "select",
    optional: true,
    options: [
      { value: "sticky", label: "field.rotation_strategy.sticky" },
      { value: "round_robin", label: "field.rotation_strategy.round_robin" },
      { value: "cache_affinity", label: "field.rotation_strategy.cache_affinity" },
    ],
  },
  { key: "sanitize_rules", label: "sanitize_rules", type: "json", optional: true },
  { key: "rewrite_rules", label: "rewrite_rules", type: "json", optional: true },
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
      { key: "flatten_system_before_cache", label: "flatten_system_before_cache", type: "boolean", optional: true },
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
      claude_ai_base_url: "https://claude.ai",
      platform_base_url: "https://platform.claude.com",
      fingerprint: DEFAULT_CLAUDECODE_FINGERPRINT,
    },
    fields: [
      { key: "base_url", label: "base_url", type: "text" },
      { key: "claude_ai_base_url", label: "claude_ai_base_url", type: "text" },
      { key: "platform_base_url", label: "platform_base_url", type: "text" },
      { key: "fingerprint", label: "fingerprint", type: "json", optional: true },
      { key: "enable_magic_cache", label: "enable_magic_cache", type: "boolean", optional: true },
      { key: "flatten_system_before_cache", label: "flatten_system_before_cache", type: "boolean", optional: true },
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
  chatgpt: {
    defaults: {
      base_url: "https://chatgpt.com",
      user_agent: "",
      temporary_chat: "true",
    },
    fields: [
      { key: "base_url", label: "base_url", type: "text" },
      { key: "user_agent", label: "user_agent", type: "text", optional: true },
      {
        key: "temporary_chat",
        label: "temporary_chat",
        type: "boolean",
        optional: true,
      },
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
    defaults: { base_url: "https://openrouter.ai/api", user_agent: "" },
    fields: [
      { key: "base_url", label: "base_url", type: "text" },
      { key: "user_agent", label: "user_agent", type: "text", optional: true },
    ],
  },
  vercel: {
    defaults: { base_url: "https://ai-gateway.vercel.sh", user_agent: "" },
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
    ],
  },
};

export const CREDENTIAL_CHANNEL_CONFIG: Record<string, ChannelCredentialConfig> = {
  openai: { fields: [{ key: "api_key", label: "api_key", type: "textarea" }] },
  anthropic: { fields: [{ key: "api_key", label: "api_key", type: "textarea" }] },
  aistudio: { fields: [{ key: "api_key", label: "api_key", type: "textarea" }] },
  vertex: {
    fields: [
      { key: "project_id", label: "project_id", type: "text" },
      { key: "client_email", label: "client_email", type: "text" },
      { key: "private_key", label: "private_key", type: "textarea" },
      { key: "private_key_id", label: "private_key_id", type: "text", optional: true },
      { key: "client_id", label: "client_id", type: "text", optional: true },
      { key: "token_uri", label: "token_uri", type: "text", optional: true },
    ],
  },
  vertexexpress: { fields: [{ key: "api_key", label: "api_key", type: "text" }] },
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
  chatgpt: {
    fields: [
      { key: "access_token", label: "access_token (chatgpt.com web session)", type: "textarea" },
      { key: "chat_req_token", label: "chat_req_token", type: "textarea", optional: true },
      { key: "proof_token", label: "proof_token", type: "textarea", optional: true },
      { key: "chat_req_token_expires_at_ms", label: "chat_req_token_expires_at_ms", type: "integer", optional: true },
      { key: "persona", label: "persona", type: "text", optional: true },
      { key: "device_id", label: "device_id", type: "text", optional: true },
    ],
  },
  nvidia: { fields: [{ key: "api_key", label: "api_key", type: "textarea" }] },
  deepseek: { fields: [{ key: "api_key", label: "api_key", type: "textarea" }] },
  groq: { fields: [{ key: "api_key", label: "api_key", type: "textarea" }] },
  openrouter: { fields: [{ key: "api_key", label: "api_key", type: "textarea" }] },
  vercel: { fields: [{ key: "api_key", label: "api_key", type: "textarea" }] },
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
  return normalizeCredentialJson(
    channel,
    buildObjectFromFields(credentialFieldsForChannel(channel), values),
  );
}

export function parseCredentialImport(
  channel: string,
  rawInput: string,
): Record<string, unknown>[] {
  const source = rawInput.trim();
  if (source === "") {
    return [];
  }

  const fullJson = tryParseJson(source);
  if (fullJson.ok) {
    return credentialsFromJsonValue(channel, fullJson.value);
  }

  const credentials: Record<string, unknown>[] = [];
  let cursor = 0;
  while (cursor < source.length) {
    const jsonStart = findLineJsonStart(source, cursor);
    if (jsonStart === -1) {
      appendRawCredentialLines(channel, source.slice(cursor), credentials);
      break;
    }

    appendRawCredentialLines(channel, source.slice(cursor, jsonStart), credentials);
    const jsonEnd = findJsonValueEnd(source, jsonStart);
    const segment = source.slice(jsonStart, jsonEnd);
    const parsed = tryParseJson(segment);
    if (!parsed.ok) {
      throw new Error(`Invalid credential JSON: ${parsed.message}`);
    }
    credentials.push(...credentialsFromJsonValue(channel, parsed.value));
    cursor = jsonEnd;
  }

  return credentials;
}

export function normalizeCredentialJson(
  channel: string,
  credential: Record<string, unknown>,
): Record<string, unknown> {
  if (channel !== "claudecode") {
    return credential;
  }
  const cookie = credential.cookie;
  if (typeof cookie !== "string") {
    return credential;
  }
  return {
    ...credential,
    cookie: normalizeClaudeCodeCookie(cookie),
  };
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

function rawCredentialForChannel(channel: string, raw: string): Record<string, unknown> {
  if (channel === "claudecode") {
    return normalizeCredentialJson(channel, { cookie: raw });
  }
  if (channel === "chatgpt") {
    return { access_token: raw };
  }
  return { api_key: raw };
}

function appendRawCredentialLines(
  channel: string,
  raw: string,
  credentials: Record<string, unknown>[],
) {
  for (const line of raw.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (trimmed !== "") {
      credentials.push(rawCredentialForChannel(channel, trimmed));
    }
  }
}

function credentialsFromJsonValue(
  channel: string,
  value: unknown,
): Record<string, unknown>[] {
  if (Array.isArray(value)) {
    return value.flatMap((item) => credentialsFromJsonValue(channel, item));
  }
  if (isPlainObject(value)) {
    return [normalizeCredentialJson(channel, value)];
  }
  if (typeof value === "string" && value.trim() !== "") {
    return [rawCredentialForChannel(channel, value.trim())];
  }
  throw new Error("Credential JSON entries must be objects or non-empty strings");
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

type JsonParseResult =
  | { ok: true; value: unknown }
  | { ok: false; message: string };

function tryParseJson(source: string): JsonParseResult {
  try {
    return { ok: true, value: JSON.parse(source) };
  } catch (error) {
    return {
      ok: false,
      message: error instanceof Error ? error.message : String(error),
    };
  }
}

function findLineJsonStart(source: string, from: number): number {
  for (let index = from; index < source.length; index += 1) {
    const char = source[index];
    if ((char === "{" || char === "[") && isFirstNonWhitespaceOnLine(source, index)) {
      return index;
    }
  }
  return -1;
}

function isFirstNonWhitespaceOnLine(source: string, index: number): boolean {
  const lineStart = source.lastIndexOf("\n", index - 1) + 1;
  return source.slice(lineStart, index).trim() === "";
}

function findJsonValueEnd(source: string, start: number): number {
  const stack: string[] = [];
  let inString = false;
  let escaped = false;

  for (let index = start; index < source.length; index += 1) {
    const char = source[index];
    if (inString) {
      if (escaped) {
        escaped = false;
      } else if (char === "\\") {
        escaped = true;
      } else if (char === "\"") {
        inString = false;
      }
      continue;
    }

    if (char === "\"") {
      inString = true;
      continue;
    }
    if (char === "{" || char === "[") {
      stack.push(char === "{" ? "}" : "]");
      continue;
    }
    if (char === "}" || char === "]") {
      const expected = stack.pop();
      if (expected !== char) {
        throw new Error("Invalid credential JSON: mismatched brackets");
      }
      if (stack.length === 0) {
        return index + 1;
      }
    }
  }

  throw new Error("Invalid credential JSON: incomplete JSON value");
}

function normalizeClaudeCodeCookie(raw: string): string {
  const trimmed = raw.trim();
  if (trimmed === "") {
    return trimmed;
  }
  const header = trimmed.replace(/^cookie:\s*/i, "");
  const sessionKey = header
    .split(";")
    .map((part) => part.trim())
    .find((part) => part.toLowerCase().startsWith("sessionkey="));
  if (!sessionKey) {
    return header;
  }
  return sessionKey.slice("sessionKey=".length).trim().replace(/^"|"$/g, "");
}

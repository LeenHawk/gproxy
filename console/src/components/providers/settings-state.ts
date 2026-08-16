import type { ChannelMeta, ChannelSettingField, EndpointKind } from "@/lib/channel-meta";
import { isValidEndpointUrl, type EndpointRow } from "./endpoint-fields";
import { assembleGenericSettings, genericSettingFields } from "./generic-settings-fields";

export interface SettingsState {
  genericSettings: Record<string, unknown>;
  baseUrl: string;
  endpoints: EndpointRow[];
  consecutiveFailures: string;
  cooldownSecs: string;
  autoRefreshModels: boolean;
  location: string;
  region: string;
  profileArn: string;
  apiVersion: string;
  enableOpenAiMagicCache: boolean;
  enableClaudeMagicCache: boolean;
  enableClaudeFableFallback: boolean;
  claudeFableFallbackModels: string[];
}

export type SettingsValidationError =
  | "base_url_required"
  | "endpoints_required"
  | "endpoints_invalid"
  | "circuit_breaker_invalid";

const AWS_CHANNELS = new Set(["aws-bedrock"]);
const OPENAI_MAGIC_CACHE_CHANNELS = new Set([
  "openai", "azure", "aws-bedrock", "codex", "vercel", "openrouter", "custom",
]);
const CLAUDE_MAGIC_CACHE_CHANNELS = new Set([
  "claudecode", "claudeapi", "azure", "aws-bedrock", "vercel", "openrouter", "custom",
]);
const CLAUDE_FALLBACK_CHANNELS = new Set([
  "claudecode", "claudeapi", "vercel", "openrouter", "custom",
]);

function objectValue(value: unknown): Record<string, unknown> | undefined {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? value as Record<string, unknown>
    : undefined;
}

function reservedField(meta: ChannelMeta | undefined, key: string): ChannelSettingField | undefined {
  return meta?.source === "external"
    ? meta.settingsFields.find((field) => field.key === key)
    : undefined;
}

function positiveInteger(value: unknown): number | undefined {
  const number = typeof value === "string" && value.trim() !== "" ? Number(value) : value;
  return typeof number === "number" && Number.isInteger(number) && number > 0
    ? number
    : undefined;
}

function endpointRows(
  value: unknown,
  meta: ChannelMeta | undefined,
  validateUrls: boolean,
): EndpointRow[] {
  const endpoints = objectValue(value);
  if (!endpoints) return [];
  const supported = new Set(meta?.endpointKinds ?? []);
  return Object.entries(endpoints)
    .filter((entry): entry is [EndpointKind, string] =>
      typeof entry[1] === "string"
        && (meta === undefined || supported.has(entry[0]))
        && (!validateUrls || isValidEndpointUrl(entry[1])),
    )
    .map(([kind, url]) => ({ kind, url }));
}

export function initSettingsState(
  settingsJson: unknown,
  meta: ChannelMeta | undefined,
): SettingsState {
  const settings = objectValue(settingsJson) ?? {};
  const baseUrlField = reservedField(meta, "base_url");
  const endpointsField = reservedField(meta, "endpoints");
  const circuitBreakerField = reservedField(meta, "circuit_breaker");
  const autoRefreshField = reservedField(meta, "auto_refresh_models");
  const persistedBreaker = objectValue(settings.circuit_breaker);
  const defaultBreaker = settings.circuit_breaker === undefined
    ? objectValue(circuitBreakerField?.default)
    : undefined;
  const validDefaultBreaker = defaultBreaker
    && positiveInteger(defaultBreaker.consecutive_failures) !== undefined
    && positiveInteger(defaultBreaker.cooldown_secs) !== undefined
    ? defaultBreaker
    : undefined;
  const breaker = persistedBreaker ?? validDefaultBreaker ?? {};
  const endpointSource = settings.endpoints === undefined
    ? endpointsField?.default
    : settings.endpoints;
  const consecutiveFailures = positiveInteger(breaker.consecutive_failures);
  const cooldownSecs = positiveInteger(breaker.cooldown_secs);

  return {
    genericSettings: { ...settings },
    baseUrl: typeof settings.base_url === "string"
      ? settings.base_url
      : typeof baseUrlField?.default === "string" ? baseUrlField.default : "",
    endpoints: endpointRows(endpointSource, meta, settings.endpoints === undefined),
    consecutiveFailures: consecutiveFailures === undefined ? "" : String(consecutiveFailures),
    cooldownSecs: cooldownSecs === undefined ? "" : String(cooldownSecs),
    autoRefreshModels: typeof settings.auto_refresh_models === "boolean"
      ? settings.auto_refresh_models
      : typeof autoRefreshField?.default === "boolean" ? autoRefreshField.default : true,
    location: typeof settings.location === "string" ? settings.location : "",
    region: typeof settings.region === "string" ? settings.region : "",
    profileArn: typeof settings.profile_arn === "string" ? settings.profile_arn : "",
    apiVersion: typeof settings.api_version === "string" ? settings.api_version : "",
    enableOpenAiMagicCache: settings.enable_openai_magic_cache === true,
    enableClaudeMagicCache: settings.enable_claude_magic_cache === true,
    enableClaudeFableFallback:
      settings.claude_fable_fallbacks === "default"
      || Array.isArray(settings.claude_fable_fallbacks),
    claudeFableFallbackModels: Array.isArray(settings.claude_fable_fallbacks)
      ? settings.claude_fable_fallbacks
          .filter((model): model is string => typeof model === "string")
          .slice(0, 3)
          .concat(["", "", ""])
          .slice(0, 3)
      : ["", "", ""],
  };
}

export function validateSettingsState(
  state: SettingsState,
  meta: ChannelMeta | undefined,
): SettingsValidationError | null {
  if (reservedField(meta, "base_url")?.required === true && !state.baseUrl.trim()) {
    return "base_url_required";
  }

  const endpointKinds = new Set(state.endpoints.map((row) => row.kind));
  const endpointsValid = endpointKinds.size === state.endpoints.length
    && state.endpoints.every((row) => row.kind !== "" && isValidEndpointUrl(row.url));
  if (!endpointsValid) return "endpoints_invalid";
  if (reservedField(meta, "endpoints")?.required === true && state.endpoints.length === 0) {
    return "endpoints_required";
  }

  const failures = positiveInteger(state.consecutiveFailures);
  const cooldown = positiveInteger(state.cooldownSecs);
  const breakerRequired = reservedField(meta, "circuit_breaker")?.required === true;
  const breakerPresent = state.consecutiveFailures.trim() !== "" || state.cooldownSecs.trim() !== "";
  if ((breakerRequired || breakerPresent) && (failures === undefined || cooldown === undefined)) {
    return "circuit_breaker_invalid";
  }
  return null;
}

export function assembleSettings(
  base: unknown,
  state: SettingsState,
  channel: string,
  meta: ChannelMeta | undefined,
): Record<string, unknown> {
  const result: Record<string, unknown> = { ...(objectValue(base) ?? {}) };
  const isExternal = meta?.source === "external";
  const isBuiltin = meta?.source === "builtin";

  if (state.baseUrl.trim()) result.base_url = state.baseUrl.trim();
  else delete result.base_url;

  const endpoints = Object.fromEntries(
    state.endpoints
      .filter((row): row is EndpointRow & { kind: EndpointKind } => row.kind !== "")
      .map((row) => [row.kind, row.url.trim()]),
  );
  if (Object.keys(endpoints).length > 0) result.endpoints = endpoints;
  else delete result.endpoints;

  const failures = positiveInteger(state.consecutiveFailures);
  const cooldown = positiveInteger(state.cooldownSecs);
  if (failures !== undefined && cooldown !== undefined) {
    result.circuit_breaker = { consecutive_failures: failures, cooldown_secs: cooldown };
  } else {
    delete result.circuit_breaker;
  }

  if (state.autoRefreshModels) {
    if (reservedField(meta, "auto_refresh_models")?.required === true) {
      result.auto_refresh_models = true;
    } else {
      delete result.auto_refresh_models;
    }
  } else {
    result.auto_refresh_models = false;
  }

  if (isBuiltin && channel === "vertex") setString(result, "location", state.location);
  if (isBuiltin && AWS_CHANNELS.has(channel)) setString(result, "region", state.region);
  if (isBuiltin && channel === "kiro") setString(result, "profile_arn", state.profileArn);
  if (isBuiltin && channel === "azure") setString(result, "api_version", state.apiVersion);

  if (isBuiltin) {
    delete result.enable_magic_cache;
    delete result.enable_beta;
    setEnabled(result, "enable_openai_magic_cache", OPENAI_MAGIC_CACHE_CHANNELS.has(channel)
      && state.enableOpenAiMagicCache);
    setEnabled(result, "enable_claude_magic_cache", CLAUDE_MAGIC_CACHE_CHANNELS.has(channel)
      && state.enableClaudeMagicCache);

    if (CLAUDE_FALLBACK_CHANNELS.has(channel) && state.enableClaudeFableFallback) {
      const models = state.claudeFableFallbackModels
        .map((model) => model.trim())
        .filter((model, index, all) => model && all.indexOf(model) === index)
        .slice(0, 3);
      result.claude_fable_fallbacks = models.length > 0 ? models : "default";
    } else {
      delete result.claude_fable_fallbacks;
    }
  }

  if (isExternal && meta) {
    return assembleGenericSettings(
      result,
      state.genericSettings,
      genericSettingFields(meta.settingsFields),
    );
  }
  if (isBuiltin && AWS_CHANNELS.has(channel) && meta) {
    const fields = genericSettingFields(meta.settingsFields)
      .filter((field) => field.key === "video_output_s3_uri");
    return assembleGenericSettings(result, state.genericSettings, fields);
  }
  return result;
}

function setString(result: Record<string, unknown>, key: string, value: string) {
  if (value.trim()) result[key] = value.trim();
  else delete result[key];
}

function setEnabled(result: Record<string, unknown>, key: string, enabled: boolean) {
  if (enabled) result[key] = true;
  else delete result[key];
}

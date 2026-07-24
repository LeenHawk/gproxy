/**
 * Structured settings fields for ProviderForm.
 * Replaces the raw-JSON `settings_json` textarea with typed controls.
 *
 * Surfaced keys:
 *   base_url          — channel-wide fallback prefix
 *   endpoints         — exact per-operation upstream URLs
 *   circuit_breaker   — all channels (both sub-fields must be filled or both omitted)
 *   auto_refresh_models — all channels (default true)
 *   location          — vertex only
 *   region            — Amazon Bedrock channel
 *   profile_arn       — kiro only
 *   api_version       — azure deployment-bound image APIs
 *   enable_openai_magic_cache — OpenAI magic-string prompt cache triggers
 *   enable_claude_magic_cache — Claude magic-string prompt cache triggers
 *   claude_fable_fallbacks — claudecode / claudeapi / vercel / openrouter / custom
 *
 * Unknown keys (e.g. tokenizer_map) are preserved via the `base` prop.
 */

import { useTranslation } from "react-i18next";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import {
  channelMeta, DEFAULT_BASE_URL, ENDPOINT_KINDS, type EndpointKind,
} from "@/lib/channel-meta";
import { EndpointFields, type EndpointRow } from "./endpoint-fields";

const OPENAI_MAGIC_CACHE_CHANNELS = new Set([
  "openai", "azure", "aws-bedrock", "codex", "vercel", "openrouter", "custom",
]);
const CLAUDE_MAGIC_CACHE_CHANNELS = new Set([
  "claudecode", "claudeapi", "azure", "aws-bedrock", "vercel", "openrouter", "custom",
]);
const CLAUDE_FALLBACK_CHANNELS = new Set([
  "claudecode", "claudeapi", "vercel", "openrouter", "custom",
]);
const AWS_CHANNELS = new Set(["aws-bedrock"]);

export interface SettingsState {
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

function objectValue(value: unknown): Record<string, unknown> | undefined {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? value as Record<string, unknown>
    : undefined;
}

export function initSettingsState(settingsJson: unknown, channel: string): SettingsState {
  const s = objectValue(settingsJson) ?? {};
  const cb = objectValue(s.circuit_breaker) ?? {};
  const supported = new Set(channelMeta(channel)?.endpointKinds ?? []);
  const endpoints = objectValue(s.endpoints);
  return {
    baseUrl: typeof s.base_url === "string" ? s.base_url : "",
    endpoints: endpoints
      ? Object.entries(endpoints)
          .filter((entry): entry is [EndpointKind, string] =>
            ENDPOINT_KINDS.includes(entry[0] as EndpointKind)
              && supported.has(entry[0] as EndpointKind)
              && typeof entry[1] === "string",
          )
          .map(([kind, url]) => ({ kind, url }))
      : [],
    consecutiveFailures:
      typeof cb.consecutive_failures === "number"
        ? String(cb.consecutive_failures)
        : "",
    cooldownSecs:
      typeof cb.cooldown_secs === "number" ? String(cb.cooldown_secs) : "",
    autoRefreshModels: s.auto_refresh_models !== false,
    location: typeof s.location === "string" ? s.location : "",
    region: typeof s.region === "string" ? s.region : "",
    profileArn: typeof s.profile_arn === "string" ? s.profile_arn : "",
    apiVersion: typeof s.api_version === "string" ? s.api_version : "",
    enableOpenAiMagicCache: s.enable_openai_magic_cache === true,
    enableClaudeMagicCache: s.enable_claude_magic_cache === true,
    enableClaudeFableFallback:
      s.claude_fable_fallbacks === "default" || Array.isArray(s.claude_fable_fallbacks),
    claudeFableFallbackModels: Array.isArray(s.claude_fable_fallbacks)
      ? s.claude_fable_fallbacks
          .filter((model): model is string => typeof model === "string")
          .slice(0, 3)
          .concat(["", "", ""])
          .slice(0, 3)
      : ["", "", ""],
  };
}

/**
 * Merge the form state back into the existing settings_json, preserving
 * unknown keys (e.g. tokenizer_map). Returns the assembled settings object.
 */
export function assembleSettings(
  base: unknown,
  state: SettingsState,
  channel: string,
): Record<string, unknown> {
  const result: Record<string, unknown> = { ...(objectValue(base) ?? {}) };

  if (state.baseUrl.trim()) {
    result.base_url = state.baseUrl.trim();
  } else {
    delete result.base_url;
  }

  const endpoints = Object.fromEntries(
    state.endpoints
      .filter((row): row is EndpointRow & { kind: EndpointKind } => row.kind !== "")
      .map((row) => [row.kind, row.url.trim()]),
  );
  if (Object.keys(endpoints).length > 0) {
    result.endpoints = endpoints;
  } else {
    delete result.endpoints;
  }

  // circuit_breaker: include only when BOTH fields are filled
  const cf = parseInt(state.consecutiveFailures, 10);
  const cs = parseInt(state.cooldownSecs, 10);
  if (!isNaN(cf) && !isNaN(cs) && state.consecutiveFailures.trim() && state.cooldownSecs.trim()) {
    result.circuit_breaker = { consecutive_failures: cf, cooldown_secs: cs };
  } else {
    delete result.circuit_breaker;
  }

  // Automatic model refresh defaults on; persist only the opt-out.
  if (state.autoRefreshModels) {
    delete result.auto_refresh_models;
  } else {
    result.auto_refresh_models = false;
  }

  // location (vertex only)
  if (channel === "vertex") {
    if (state.location.trim()) {
      result.location = state.location.trim();
    } else {
      delete result.location;
    }
  }

  if (AWS_CHANNELS.has(channel)) {
    if (state.region.trim()) {
      result.region = state.region.trim();
    } else {
      delete result.region;
    }
  }

  // profile_arn (kiro only)
  if (channel === "kiro") {
    if (state.profileArn.trim()) {
      result.profile_arn = state.profileArn.trim();
    } else {
      delete result.profile_arn;
    }
  }

  if (channel === "azure") {
    if (state.apiVersion.trim()) {
      result.api_version = state.apiVersion.trim();
    } else {
      delete result.api_version;
    }
  }

  delete result.enable_magic_cache;
  if (OPENAI_MAGIC_CACHE_CHANNELS.has(channel)) {
    if (state.enableOpenAiMagicCache) {
      result.enable_openai_magic_cache = true;
    } else {
      delete result.enable_openai_magic_cache;
    }
  } else {
    delete result.enable_openai_magic_cache;
  }
  if (CLAUDE_MAGIC_CACHE_CHANNELS.has(channel)) {
    if (state.enableClaudeMagicCache) {
      result.enable_claude_magic_cache = true;
    } else {
      delete result.enable_claude_magic_cache;
    }
  } else {
    delete result.enable_claude_magic_cache;
  }

  if (CLAUDE_FALLBACK_CHANNELS.has(channel)) {
    if (state.enableClaudeFableFallback) {
      const models = state.claudeFableFallbackModels
        .map((model) => model.trim())
        .filter((model, index, all) => model && all.indexOf(model) === index)
        .slice(0, 3);
      result.claude_fable_fallbacks = models.length > 0 ? models : "default";
    } else {
      delete result.claude_fable_fallbacks;
    }
  } else {
    delete result.claude_fable_fallbacks;
  }

  return result;
}

interface SettingsFieldsProps {
  channel: string;
  state: SettingsState;
  onChange: (next: Partial<SettingsState>) => void;
}

export function SettingsFields({ channel, state, onChange }: SettingsFieldsProps) {
  const { t } = useTranslation("providers");
  const defaultUrl = DEFAULT_BASE_URL[channel];
  const isCustom = channel === "custom";

  return (
    <div className="grid gap-3">
      <div className="grid gap-2">
        <Label htmlFor="sf-base-url">{t("fields.baseUrl")}</Label>
        <Input
          id="sf-base-url"
          value={state.baseUrl}
          onChange={(event) => onChange({ baseUrl: event.target.value })}
          placeholder={isCustom ? t("form.baseUrlOrEndpointRequired") : defaultUrl ?? t("form.baseUrlHint")}
        />
        {!isCustom && (
          <p className="text-xs text-muted-foreground">{t("form.baseUrlHint")}</p>
        )}
      </div>

      <EndpointFields
        channel={channel}
        rows={state.endpoints}
        onChange={(endpoints) => onChange({ endpoints })}
      />

      {AWS_CHANNELS.has(channel) && (
        <div className="grid gap-2">
          <Label htmlFor="sf-region">{t("fields.region")}</Label>
          <Input
            id="sf-region"
            value={state.region}
            onChange={(event) => onChange({ region: event.target.value })}
            placeholder="us-east-1"
          />
          <p className="text-xs text-muted-foreground">{t("form.bedrockRegionHint")}</p>
        </div>
      )}

      {channel === "azure" && (
        <div className="grid gap-2">
          <Label htmlFor="sf-api-version">{t("fields.apiVersion")}</Label>
          <Input
            id="sf-api-version"
            value={state.apiVersion}
            onChange={(event) => onChange({ apiVersion: event.target.value })}
            placeholder="2025-04-01-preview"
          />
          <p className="text-xs text-muted-foreground">{t("form.apiVersionHint")}</p>
        </div>
      )}

      {/* circuit breaker */}
      <div className="grid gap-2">
        <Label>{t("fields.circuitBreaker")}</Label>
        <div className="grid grid-cols-2 gap-2">
          <div className="grid gap-1">
            <Label htmlFor="sf-cf" className="text-xs font-normal text-muted-foreground">
              {t("fields.consecutiveFailures")}
            </Label>
            <Input
              id="sf-cf"
              type="number"
              min={1}
              value={state.consecutiveFailures}
              onChange={(e) => onChange({ consecutiveFailures: e.target.value })}
              placeholder="5"
            />
          </div>
          <div className="grid gap-1">
            <Label htmlFor="sf-cs" className="text-xs font-normal text-muted-foreground">
              {t("fields.cooldownSecs")}
            </Label>
            <Input
              id="sf-cs"
              type="number"
              min={1}
              value={state.cooldownSecs}
              onChange={(e) => onChange({ cooldownSecs: e.target.value })}
              placeholder="60"
            />
          </div>
        </div>
      </div>

      <div className="grid gap-1">
        <div className="flex items-center justify-between gap-4">
          <Label htmlFor="sf-auto-refresh-models">{t("fields.autoRefreshModels")}</Label>
          <Switch
            id="sf-auto-refresh-models"
            checked={state.autoRefreshModels}
            onCheckedChange={(v) => onChange({ autoRefreshModels: v })}
          />
        </div>
        <p className="text-xs text-muted-foreground">{t("form.autoRefreshModelsHint")}</p>
      </div>

      {/* vertex: location */}
      {channel === "vertex" && (
        <div className="grid gap-2">
          <Label htmlFor="sf-location">{t("fields.location")}</Label>
          <Input
            id="sf-location"
            value={state.location}
            onChange={(e) => onChange({ location: e.target.value })}
            placeholder="us-central1"
          />
        </div>
      )}

      {/* kiro: profile_arn */}
      {channel === "kiro" && (
        <div className="grid gap-2">
          <Label htmlFor="sf-arn">{t("fields.profileArn")}</Label>
          <Input
            id="sf-arn"
            value={state.profileArn}
            onChange={(e) => onChange({ profileArn: e.target.value })}
            placeholder="arn:aws:…"
          />
        </div>
      )}

      {OPENAI_MAGIC_CACHE_CHANNELS.has(channel) && (
        <div className="grid gap-1">
          <div className="flex items-center justify-between gap-4">
            <Label htmlFor="sf-openai-magic-cache">{t("fields.enableOpenAiMagicCache")}</Label>
            <Switch
              id="sf-openai-magic-cache"
              checked={state.enableOpenAiMagicCache}
              onCheckedChange={(v) => onChange({ enableOpenAiMagicCache: v })}
            />
          </div>
          <p className="text-xs text-muted-foreground">{t("form.enableOpenAiMagicCacheHint")}</p>
        </div>
      )}
      {CLAUDE_MAGIC_CACHE_CHANNELS.has(channel) && (
        <div className="grid gap-1">
          <div className="flex items-center justify-between gap-4">
            <Label htmlFor="sf-claude-magic-cache">{t("fields.enableClaudeMagicCache")}</Label>
            <Switch
              id="sf-claude-magic-cache"
              checked={state.enableClaudeMagicCache}
              onCheckedChange={(v) => onChange({ enableClaudeMagicCache: v })}
            />
          </div>
          <p className="text-xs text-muted-foreground">{t("form.enableClaudeMagicCacheHint")}</p>
        </div>
      )}
      {CLAUDE_FALLBACK_CHANNELS.has(channel) && (
        <div className="grid gap-1">
          <div className="flex items-center justify-between gap-4">
            <Label htmlFor="sf-claude-fable-fallback">
              {t("fields.enableClaudeFableFallback")}
            </Label>
            <Switch
              id="sf-claude-fable-fallback"
              checked={state.enableClaudeFableFallback}
              onCheckedChange={(v) => onChange({ enableClaudeFableFallback: v })}
            />
          </div>
          <p className="text-xs text-muted-foreground">
            {t("form.enableClaudeFableFallbackHint")}
          </p>
          {state.enableClaudeFableFallback && (
            <div className="grid gap-2 pt-1">
              {state.claudeFableFallbackModels.map((model, index) => (
                <Input
                  key={index}
                  aria-label={t("form.claudeFableFallbackModel", { index: index + 1 })}
                  value={model}
                  onChange={(event) => {
                    const models = [...state.claudeFableFallbackModels];
                    models[index] = event.target.value;
                    onChange({ claudeFableFallbackModels: models });
                  }}
                  placeholder={index === 0
                    ? t("form.claudeFableFallbackDefault")
                    : t("form.claudeFableFallbackModel", { index: index + 1 })}
                />
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

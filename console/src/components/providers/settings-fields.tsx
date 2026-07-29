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
  DEFAULT_BASE_URL, type ChannelMeta,
} from "@/lib/channel-meta";
import { EndpointFields } from "./endpoint-fields";
import { genericSettingFields, GenericSettingsFields } from "./generic-settings-fields";
import type { SettingsState } from "./settings-state";

export {
  assembleSettings, initSettingsState, validateSettingsState,
  type SettingsState, type SettingsValidationError,
} from "./settings-state";

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
interface SettingsFieldsProps {
  channel: string;
  meta?: ChannelMeta;
  state: SettingsState;
  onChange: (next: Partial<SettingsState>) => void;
}

export function SettingsFields({ channel, meta, state, onChange }: SettingsFieldsProps) {
  const { t } = useTranslation("providers");
  const resolvedMeta = meta;
  const isBuiltin = resolvedMeta?.source === "builtin";
  const defaultUrl = isBuiltin ? DEFAULT_BASE_URL[channel] : undefined;
  const isCustom = isBuiltin && channel === "custom";
  const baseUrlField = resolvedMeta?.source === "external"
    ? resolvedMeta.settingsFields.find((field) => field.key === "base_url")
    : undefined;
  const autoRefreshField = resolvedMeta?.source === "external"
    ? resolvedMeta.settingsFields.find((field) => field.key === "auto_refresh_models")
    : undefined;
  const endpointsField = resolvedMeta?.source === "external"
    ? resolvedMeta.settingsFields.find((field) => field.key === "endpoints")
    : undefined;
  const circuitBreakerField = resolvedMeta?.source === "external"
    ? resolvedMeta.settingsFields.find((field) => field.key === "circuit_breaker")
    : undefined;

  return (
    <div className="grid gap-3">
      <div className="grid gap-2">
        <Label htmlFor="sf-base-url">{baseUrlField?.label ?? t("fields.baseUrl")}</Label>
        <Input
          id="sf-base-url"
          type="url"
          value={state.baseUrl}
          onChange={(event) => onChange({ baseUrl: event.target.value })}
          required={baseUrlField?.required}
          placeholder={baseUrlField?.placeholder
            ?? (isCustom ? t("form.baseUrlOrEndpointRequired") : defaultUrl ?? t("form.baseUrlHint"))}
        />
        {!isCustom && (
          <p className="text-xs text-muted-foreground">{t("form.baseUrlHint")}</p>
        )}
      </div>

      <EndpointFields
        endpointKinds={resolvedMeta?.endpointKinds ?? []}
        rows={state.endpoints}
        required={endpointsField?.required === true}
        onChange={(endpoints) => onChange({ endpoints })}
      />

      {resolvedMeta?.source === "external" && (
        <GenericSettingsFields
          fields={genericSettingFields(resolvedMeta.settingsFields)}
          values={state.genericSettings}
          onChange={(genericSettings) => onChange({ genericSettings })}
        />
      )}

      {isBuiltin && AWS_CHANNELS.has(channel) && (
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

      {isBuiltin && channel === "azure" && (
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
        <Label>
          {t("fields.circuitBreaker")}
          {circuitBreakerField?.required === true ? ` (${t("form.required")})` : ""}
        </Label>
        <div className="grid grid-cols-2 gap-2">
          <div className="grid gap-1">
            <Label htmlFor="sf-cf" className="text-xs font-normal text-muted-foreground">
              {t("fields.consecutiveFailures")}
            </Label>
            <Input
              id="sf-cf"
              type="number"
              min={1}
              required={circuitBreakerField?.required === true}
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
              required={circuitBreakerField?.required === true}
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
            aria-required={autoRefreshField?.required === true}
            onCheckedChange={(v) => onChange({ autoRefreshModels: v })}
          />
        </div>
        <p className="text-xs text-muted-foreground">{t("form.autoRefreshModelsHint")}</p>
      </div>

      {/* vertex: location */}
      {isBuiltin && channel === "vertex" && (
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
      {isBuiltin && channel === "kiro" && (
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

      {isBuiltin && OPENAI_MAGIC_CACHE_CHANNELS.has(channel) && (
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
      {isBuiltin && CLAUDE_MAGIC_CACHE_CHANNELS.has(channel) && (
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
      {isBuiltin && CLAUDE_FALLBACK_CHANNELS.has(channel) && (
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

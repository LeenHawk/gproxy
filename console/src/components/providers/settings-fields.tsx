/**
 * Structured settings fields for ProviderForm.
 * Replaces the raw-JSON `settings_json` textarea with typed controls.
 *
 * Surfaced keys:
 *   base_url          — all channels (optional override; required for "custom")
 *   circuit_breaker   — all channels (both sub-fields must be filled or both omitted)
 *   location          — vertex only
 *   profile_arn       — kiro only
 *   enable_magic_cache — claudecode / claudeapi / vercel / openrouter (magic-string prompt cache triggers)
 *
 * Unknown keys (e.g. tokenizer_map) are preserved via the `base` prop.
 */

import { useTranslation } from "react-i18next";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { DEFAULT_BASE_URL } from "@/lib/channel-meta";

// Channels whose backend honors the magic-string cache triggers on Claude-format bodies.
const MAGIC_CACHE_CHANNELS = new Set(["claudecode", "claudeapi", "vercel", "openrouter"]);

export interface SettingsState {
  baseUrl: string;
  consecutiveFailures: string;
  cooldownSecs: string;
  location: string;
  profileArn: string;
  enableMagicCache: boolean;
}

export function initSettingsState(settingsJson: unknown): SettingsState {
  const s = (settingsJson ?? {}) as Record<string, unknown>;
  const cb = (s.circuit_breaker ?? {}) as Record<string, unknown>;
  return {
    baseUrl: typeof s.base_url === "string" ? s.base_url : "",
    consecutiveFailures:
      typeof cb.consecutive_failures === "number"
        ? String(cb.consecutive_failures)
        : "",
    cooldownSecs:
      typeof cb.cooldown_secs === "number" ? String(cb.cooldown_secs) : "",
    location: typeof s.location === "string" ? s.location : "",
    profileArn: typeof s.profile_arn === "string" ? s.profile_arn : "",
    enableMagicCache: s.enable_magic_cache === true,
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
  const result: Record<string, unknown> = { ...(base as Record<string, unknown> ?? {}) };

  // base_url: include only if non-empty
  if (state.baseUrl.trim()) {
    result.base_url = state.baseUrl.trim();
  } else {
    delete result.base_url;
  }

  // circuit_breaker: include only when BOTH fields are filled
  const cf = parseInt(state.consecutiveFailures, 10);
  const cs = parseInt(state.cooldownSecs, 10);
  if (!isNaN(cf) && !isNaN(cs) && state.consecutiveFailures.trim() && state.cooldownSecs.trim()) {
    result.circuit_breaker = { consecutive_failures: cf, cooldown_secs: cs };
  } else {
    delete result.circuit_breaker;
  }

  // location (vertex only)
  if (channel === "vertex") {
    if (state.location.trim()) {
      result.location = state.location.trim();
    } else {
      delete result.location;
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

  // enable_magic_cache (Claude-capable channels)
  if (MAGIC_CACHE_CHANNELS.has(channel)) {
    if (state.enableMagicCache) {
      result.enable_magic_cache = true;
    } else {
      delete result.enable_magic_cache;
    }
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
      {/* base_url */}
      <div className="grid gap-2">
        <Label htmlFor="sf-base-url">{t("fields.baseUrl")}</Label>
        <Input
          id="sf-base-url"
          value={state.baseUrl}
          onChange={(e) => onChange({ baseUrl: e.target.value })}
          placeholder={
            isCustom
              ? t("form.baseUrlRequired")
              : defaultUrl
                ? defaultUrl
                : t("form.baseUrlHint")
          }
        />
        {!isCustom && (
          <p className="text-xs text-muted-foreground">{t("form.baseUrlHint")}</p>
        )}
      </div>

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

      {/* Claude-capable channels: magic-string prompt cache triggers */}
      {MAGIC_CACHE_CHANNELS.has(channel) && (
        <div className="grid gap-1">
          <div className="flex items-center justify-between gap-4">
            <Label htmlFor="sf-magic-cache">{t("fields.enableMagicCache")}</Label>
            <Switch
              id="sf-magic-cache"
              checked={state.enableMagicCache}
              onCheckedChange={(v) => onChange({ enableMagicCache: v })}
            />
          </div>
          <p className="text-xs text-muted-foreground">{t("form.enableMagicCacheHint")}</p>
        </div>
      )}
    </div>
  );
}

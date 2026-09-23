import type { SettingsDto } from "@/generated/sdk"
import type { SettingsWrite } from "@/api/settings"
export type SettingField = {
  group: "instance" | "logging"
  name: string
  kind: "text" | "number" | "switch" | "list" | "choice" | "profile" | "vocabulary" | "allowlist" | "proxy"
  min?: number
  nullable?: boolean
  options?: Array<string>
}
const instance = (
  name: keyof SettingsDto["instance"],
  kind: SettingField["kind"],
  extra: Partial<SettingField> = {},
): SettingField => ({ group: "instance", name, kind, ...extra })
const logging = (
  name: keyof SettingsDto["logging"],
  kind: SettingField["kind"],
  extra: Partial<SettingField> = {},
): SettingField => ({ group: "logging", name, kind, ...extra })
export const groups = [
  {
    id: "general",
    fields: [
      instance("instanceName", "text"),
      instance("oauthClientAllowlist", "allowlist", { nullable: true }),
      instance("portalRecentRequestsEnabled", "switch"),
    ],
  },
  {
    id: "network",
    fields: [
      instance("proxy", "proxy", { nullable: true }),
      instance("connectionProfileId", "profile", { nullable: true }),
      instance("corsOrigins", "list"),
      instance("trustedProxies", "list"),
    ],
  },
  {
    id: "execution",
    fields: [
      instance("maxAttempts", "number", { min: 1 }),
      instance("requestTimeoutMs", "number", { min: 1 }),
      instance("streamIdleTimeoutMs", "number", { min: 1 }),
      instance("maxRequestBodyBytes", "number", { min: 1 }),
      instance("maxResponseBodyBytes", "number", { min: 1 }),
      instance("maxStreamEventBytes", "number", { min: 1 }),
      instance("maxWsFrameBytes", "number", { min: 1 }),
      instance("maxMultipartParts", "number", { min: 1 }),
      instance("enableSettlement", "switch"),
      instance("enableUsage", "switch"),
    ],
  },
  {
    id: "logging",
    fields: [
      logging("logLevel", "choice", { options: ["off", "error", "warn", "info", "debug", "trace"] }),
      logging("logFormat", "choice", { options: ["text", "json"] }),
      logging("enableTracing", "switch"),
      logging("enableDownstreamLog", "switch"),
      logging("enableDownstreamLogBody", "switch"),
      logging("enableUpstreamLog", "switch"),
      logging("enableUpstreamLogBody", "switch"),
      logging("disableLogRedaction", "switch"),
      logging("requestHeaderBlacklist", "list"),
      logging("responseHeaderBlacklist", "list"),
      logging("queryParameterBlacklist", "list"),
    ],
  },
  {
    id: "tokenizer",
    fields: [
      instance("enableTokenizerVocabs", "switch"),
      instance("enableTokenizerDownload", "switch"),
      instance("defaultVocabularyFileId", "vocabulary", { nullable: true }),
    ],
  },
  {
    id: "maintenance",
    fields: [
      instance("retentionDays", "number", { nullable: true, min: 1 }),
      instance("maxDatabaseSizeMb", "number", { nullable: true, min: 0 }),
      instance("updateChannel", "choice", { nullable: true, options: ["dev", "beta", "release"] }),
      instance("enableAutoUpdateCheck", "switch"),
    ],
  },
] as const

/** Send only edited fields; untouched settings and a saved token stay untouched. */
export function settingsPatch(
  original: SettingsDto,
  draft: SettingsDto,
  token: string | null | undefined,
): SettingsWrite {
  const patch: SettingsWrite = {}
  for (const group of groups)
    for (const field of group.fields) {
      const before = original[field.group] as Record<string, unknown>
      const after = draft[field.group] as Record<string, unknown>
      if (JSON.stringify(before[field.name]) !== JSON.stringify(after[field.name])) {
        patch[field.group] ??= {}
        Object.assign(patch[field.group]!, { [field.name]: after[field.name] })
      }
    }
  if (token !== undefined) patch.instance = { ...patch.instance, tokenizerAuthToken: token }
  return patch
}

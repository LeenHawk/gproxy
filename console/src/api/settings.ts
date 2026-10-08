import { api, json } from "@/api/client"
import type { InstanceSettingsPatch, LoggingSettingsPatch, SettingsDto, VocabularyDto } from "@/generated/sdk"

export const SETTINGS_ACCESS = "configuration.settings"
export const SETTINGS_KEY = ["admin", "settings"] as const
export const INFO_KEY = ["instance-info"] as const
export type InstanceInfo = { instanceName: string; version: string; hash: string; fontManagement?: boolean }
export type SettingsWrite = {
  instance?: Partial<InstanceSettingsPatch>
  logging?: Partial<LoggingSettingsPatch>
}
export const readSettings = () => api<SettingsDto>("/admin/api/settings")
export const saveSettings = (patch: SettingsWrite) =>
  api<SettingsDto>("/admin/api/settings", json("PATCH", patch))
export const instanceInfo = () => api<InstanceInfo>("/info")
export const vocabularies = () => api<Array<VocabularyDto>>("/admin/api/tokenizer-vocabs")

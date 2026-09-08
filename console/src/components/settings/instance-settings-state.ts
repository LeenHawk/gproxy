export const INSTANCE_SETTINGS_FORM_ID = "instance-settings-form"
export const INSTANCE_SETTINGS_MUTATION_KEY = ["instance-settings", "save"] as const

export const SETTINGS_SECTIONS = ["runtime", "network", "logs", "access", "maintenance"] as const
export type SettingsSection = typeof SETTINGS_SECTIONS[number]

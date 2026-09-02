export type VariantProtocol = "claude" | "openai_responses" | "openai_chat" | "gemini"
export type VariantAction = { path: string; value: unknown }
export type VariantPresetEntry = { suffix: string; label: string; actions: Array<VariantAction> }
export type VariantPresetGroup = { key: string; label: string; entries: Array<VariantPresetEntry> }

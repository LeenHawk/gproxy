import type { FormField } from "@/components/record-form"

export const authKinds = [
  { value: "api_key", label: "API Key" },
  { value: "oauth", label: "OAuth" },
  { value: "cookie", label: "Cookie" },
]

export const credentialFields: Array<FormField> = [
  { name: "label", kind: "text", nullable: true },
  { name: "authKind", kind: "select", required: true, createOnly: true, choices: authKinds },
  { name: "secret", kind: "json", required: true, parse: (text, values) => {
    if (values.authKind !== "api_key") return JSON.parse(text) as unknown
    const input = text.trim()
    // Structured input must remain valid JSON; a malformed object is not a key.
    if (/^[[{"]/.test(input)) {
      const parsed: unknown = JSON.parse(input)
      if (typeof parsed !== "string") return parsed
      text = parsed
    }
    const key = text.trim()
    if (!key || /\s/.test(key)) throw new Error("Invalid API key")
    return { api_key: key }
  } },
  { name: "proxy", kind: "proxy", nullable: true },
  { name: "connectionProfileId", kind: "text", nullable: true },
  { name: "expiresAtMs", kind: "datetime", nullable: true },
  { name: "metadata", kind: "json" },
  { name: "enabled", kind: "switch" },
]

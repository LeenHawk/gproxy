import type { FormField } from "@/components/record-form"

export const authKinds = [
  { value: "api_key", label: "API Key" },
  { value: "oauth", label: "OAuth" },
  { value: "cookie", label: "Cookie" },
]

export const credentialFields: Array<FormField> = [
  { name: "label", kind: "text", nullable: true },
  { name: "authKind", kind: "select", required: true, choices: authKinds },
  { name: "secret", kind: "json", required: true },
  { name: "connectionProfileId", kind: "text", nullable: true },
  { name: "expiresAtMs", kind: "datetime", nullable: true },
  { name: "metadata", kind: "json" },
  { name: "enabled", kind: "switch" },
]

import type { ChannelDescriptor } from "@/generated/sdk"
import type { FormField } from "@/components/record-form"

export function providerFields(channels: Array<ChannelDescriptor>): Array<FormField> {
  return [
    { name: "name", kind: "text", required: true },
    { name: "channel", kind: "select", required: true, choices: channels.map((channel) => ({ value: channel.id, label: channel.displayName })) },
    { name: "baseUrl", kind: "text", nullable: true },
    { name: "connectionProfileId", kind: "text", nullable: true },
    { name: "config", kind: "json" },
    { name: "enabled", kind: "switch" },
  ]
}

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

export const modelFields: Array<FormField> = [
  { name: "upstreamName", kind: "text", required: true },
  { name: "modelId", kind: "text", nullable: true },
  { name: "metadata", kind: "json" },
  { name: "enabled", kind: "switch" },
]

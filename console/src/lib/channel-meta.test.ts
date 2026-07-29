import { describe, expect, it } from "vitest";
import type { ChannelCatalogDto } from "@/api/channels";
import { mergeChannelCatalog } from "./channel-meta";

function catalogEntry(overrides: Partial<ChannelCatalogDto> = {}): ChannelCatalogDto {
  return {
    source: "builtin",
    id: "openai",
    display_name: "OpenAI",
    provider_family: "open_ai",
    credential_family: "api_key",
    login_modes: [],
    settings_fields: [],
    secret_template: { api_key: "" },
    endpoint_kinds: [],
    usage: false,
    ...overrides,
  };
}

describe("mergeChannelCatalog", () => {
  it("keeps a successful empty remote catalog empty", () => {
    expect(mergeChannelCatalog([])).toEqual([]);
  });

  it("keeps runtime capabilities authoritative for built-in channels", () => {
    const [merged] = mergeChannelCatalog([catalogEntry({
      display_name: "OpenAI Platform",
      provider_family: "gemini",
      credential_family: "oauth_tokens",
      login_modes: ["cookie"],
      settings_fields: [{ key: "runtime_only", control: "boolean", required: true }],
      secret_template: { runtime_token: "" },
      endpoint_kinds: ["external_kind"],
      usage: true,
    })]);

    expect(merged.displayName).toBe("OpenAI Platform");
    expect(merged.providerFamily).toBe("gemini");
    expect(merged.family).toBe("oauth_tokens");
    expect(merged.loginModes).toEqual(["cookie"]);
    expect(merged.settingsFields).toEqual([
      { key: "runtime_only", control: "boolean", required: true },
    ]);
    expect(merged.secretTemplate).toEqual({ runtime_token: "" });
    expect(merged.endpointKinds).toEqual(["external_kind"]);
    expect(merged.usage).toBe(true);
  });

  it("adds only UI hints to authoritative built-in metadata", () => {
    const [merged] = mergeChannelCatalog([catalogEntry({
      id: "aws-bedrock",
      display_name: "Bedrock Runtime",
      credential_family: "oauth_tokens",
    })]);

    expect(merged.family).toBe("oauth_tokens");
    expect(merged.hintKey).toBe("bedrockApiKeyHint");
  });

  it("normalizes external channels entirely from remote metadata", () => {
    const field = { key: "region", control: "text" as const, default: "us-east" };
    const [merged] = mergeChannelCatalog([catalogEntry({
      source: "external",
      id: "acme",
      display_name: "Acme Gateway",
      provider_family: "claude",
      credential_family: "oauth_tokens",
      login_modes: ["device"],
      settings_fields: [field],
      secret_template: { token: "" },
      endpoint_kinds: ["acme_messages"],
      usage: true,
    })]);

    expect(merged).toMatchObject({
      id: "acme",
      displayName: "Acme Gateway",
      source: "external",
      providerFamily: "claude",
      family: "oauth_tokens",
      loginModes: ["device"],
      settingsFields: [field],
      secretTemplate: { token: "" },
      endpointKinds: ["acme_messages"],
      usage: true,
    });
  });
});

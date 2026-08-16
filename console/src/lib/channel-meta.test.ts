import { describe, expect, it } from "vitest";
import type { ChannelCatalogDto } from "@/api/channels";
import { DEFAULT_BASE_URL, channelMeta, mergeChannelCatalog } from "./channel-meta";

function catalogEntry(overrides: Partial<ChannelCatalogDto> = {}): ChannelCatalogDto {
  return {
    source: "builtin",
    id: "openai",
    display_name: "OpenAI",
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
      credential_family: "oauth_tokens",
      login_modes: ["cookie"],
      settings_fields: [{ key: "runtime_only", control: "boolean", required: true }],
      secret_template: { runtime_token: "" },
      endpoint_kinds: ["external_kind"],
      usage: true,
    })]);

    expect(merged.displayName).toBe("OpenAI Platform");
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
      family: "oauth_tokens",
      loginModes: ["device"],
      settingsFields: [field],
      secretTemplate: { token: "" },
      endpointKinds: ["acme_messages"],
      usage: true,
    });
  });
});

describe("built-in media endpoint fallback metadata", () => {
  it("exposes each channel's implemented video surface", () => {
    for (const id of ["openai", "azure", "custom"]) {
      expect(channelMeta(id)?.endpointKinds).toContain("openai_video_create");
      expect(channelMeta(id)?.endpointKinds).toContain("openai_video_content");
    }
    expect(channelMeta("openrouter")?.endpointKinds).toEqual(expect.arrayContaining([
      "openai_video_create", "openai_video_retrieve", "openai_video_content",
    ]));
    expect(channelMeta("xai")?.endpointKinds).toEqual(expect.arrayContaining([
      "openai_video_create", "openai_video_retrieve", "openai_video_edit", "openai_video_extend",
    ]));
    for (const id of ["aistudio", "vertex", "aws-bedrock"]) {
      expect(channelMeta(id)?.endpointKinds).toEqual(expect.arrayContaining([
        "openai_video_create", "openai_video_retrieve",
      ]));
    }
  });

  it("keeps image and audio fallback declarations aligned", () => {
    expect(channelMeta("codex")?.endpointKinds).toEqual(expect.arrayContaining([
      "image_generations", "image_edits",
    ]));
    expect(channelMeta("custom")?.endpointKinds).toEqual(expect.arrayContaining([
      "openai_audio_speech", "openai_audio_transcriptions", "openai_audio_translations",
    ]));
    expect(channelMeta("dashscope")?.endpointKinds).not.toContain("openai_audio_speech");
    expect(channelMeta("openrouter")?.endpointKinds).toContain("image_generations");
  });
});

describe("Kimi API fallback metadata", () => {
  it("uses the China platform and declares only native public surfaces", () => {
    expect(DEFAULT_BASE_URL.kimiapi).toBe("https://api.moonshot.cn");
    expect(channelMeta("kimiapi")).toMatchObject({
      displayName: "Kimi API",
      family: "api_key",
      endpointKinds: ["openai_list_models", "openai_chat_completions"],
    });
  });
});

describe("Cline fallback metadata", () => {
  it("treats pasted credentials as API keys while retaining device login", () => {
    expect(channelMeta("cline")).toMatchObject({
      family: "api_key",
      loginModes: ["device"],
      secretTemplate: { api_key: "" },
    });
  });
});

describe("Kimi Code fallback metadata", () => {
  it("uses the managed coding endpoint and device OAuth", () => {
    expect(DEFAULT_BASE_URL.kimicode).toBe("https://api.kimi.com/coding/v1");
    expect(channelMeta("kimicode")).toMatchObject({
      displayName: "Kimi Code",
      family: "oauth_tokens",
      loginModes: ["device"],
      usage: true,
    });
    expect(channelMeta("kimicode")?.endpointKinds).toEqual(expect.arrayContaining([
      "openai_list_models", "openai_chat_completions", "usage",
    ]));
  });
});

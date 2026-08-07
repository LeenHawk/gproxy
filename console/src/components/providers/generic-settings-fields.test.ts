import { describe, expect, it } from "vitest";
import type { ChannelMeta, ChannelSettingField } from "@/lib/channel-meta";
import {
  assembleGenericSettings,
  genericSettingFields,
} from "./generic-settings-fields";
import {
  assembleSettings, initSettingsState, validateSettingsState,
} from "./settings-fields";

const externalMeta: ChannelMeta = {
  id: "example-openai",
  displayName: "Example OpenAI",
  source: "external",
  family: "api_key",
  loginModes: [],
  settingsFields: [{
    key: "base_url",
    control: "url",
    required: false,
    default: "https://api.openai.com",
  }],
  usage: false,
  endpointKinds: ["openai_responses"],
  secretTemplate: { api_key: "" },
};

describe("external settings", () => {
  it("lets the shared base_url control own a metadata collision", () => {
    const state = initSettingsState(
      { base_url: "https://old.example", untouched: true },
      externalMeta,
    );
    state.baseUrl = " https://new.example/v1 ";

    expect(genericSettingFields(externalMeta.settingsFields)).toEqual([]);
    expect(assembleSettings(
      { base_url: "https://old.example", untouched: true },
      state,
      externalMeta.id,
      externalMeta,
    )).toEqual({ base_url: "https://new.example/v1", untouched: true });
  });

  it("uses an external base_url default in the shared control", () => {
    const state = initSettingsState({}, externalMeta);
    expect(state.baseUrl).toBe("https://api.openai.com");
    expect(assembleSettings({}, state, externalMeta.id, externalMeta)).toEqual({
      base_url: "https://api.openai.com",
    });
  });

  it("applies and validates reserved object defaults", () => {
    const meta: ChannelMeta = {
      ...externalMeta,
      settingsFields: [
        { key: "base_url", control: "url", required: true, default: "https://default.example" },
        {
          key: "endpoints",
          control: "text",
          required: true,
          default: {
            openai_responses: "https://default.example/v1/responses",
            unsupported: "https://default.example/unsupported",
          },
        },
        {
          key: "circuit_breaker",
          control: "text",
          required: true,
          default: { consecutive_failures: 4, cooldown_secs: 30 },
        },
      ],
    };
    const base = { untouched: { enabled: true } };
    const state = initSettingsState(base, meta);

    expect(state.endpoints).toEqual([{
      kind: "openai_responses", url: "https://default.example/v1/responses",
    }]);
    expect(state.consecutiveFailures).toBe("4");
    expect(state.cooldownSecs).toBe("30");
    expect(validateSettingsState(state, meta)).toBeNull();
    expect(assembleSettings(base, state, meta.id, meta)).toEqual({
      untouched: { enabled: true },
      base_url: "https://default.example",
      endpoints: { openai_responses: "https://default.example/v1/responses" },
      circuit_breaker: { consecutive_failures: 4, cooldown_secs: 30 },
    });

    state.endpoints = [];
    expect(validateSettingsState(state, meta)).toBe("endpoints_required");
    state.endpoints = [{ kind: "openai_responses", url: "https://default.example/v1/responses" }];
    state.consecutiveFailures = "0";
    expect(validateSettingsState(state, meta)).toBe("circuit_breaker_invalid");
  });

  it("filters every reserved key and duplicate generic declaration", () => {
    const fields: ChannelSettingField[] = [
      { key: "base_url", control: "url" },
      { key: "endpoints", control: "text" },
      { key: "circuit_breaker", control: "text" },
      { key: "auto_refresh_models", control: "boolean" },
      { key: "region", control: "text" },
      { key: "region", control: "string_list" },
    ];
    expect(genericSettingFields(fields).map((field) => field.key)).toEqual(["region"]);
  });

  it("does not apply ID-specific built-in settings to an external replacement", () => {
    const meta: ChannelMeta = {
      ...externalMeta,
      id: "aws-bedrock",
      settingsFields: [{ key: "region", control: "text" }],
    };
    const state = initSettingsState({ region: "old" }, meta);
    state.region = "built-in-control";
    state.genericSettings.region = "external-control";
    expect(assembleSettings({}, state, meta.id, meta)).toEqual({
      region: "external-control",
    });
  });

  it("serializes a required Boolean as present even when it is false", () => {
    const required: ChannelSettingField = {
      key: "strict_mode", control: "boolean", required: true,
    };
    expect(assembleGenericSettings({}, {}, [required])).toEqual({ strict_mode: false });
    expect(assembleGenericSettings({}, { strict_mode: false }, [required])).toEqual({
      strict_mode: false,
    });
  });
});

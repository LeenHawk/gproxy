import { describe, expect, it } from "vitest";

import {
  buildChannelSettingsJson,
  buildCredentialJson,
  credentialFieldsForChannel,
  defaultSettingsForChannel,
  settingsFieldsForChannel,
  settingsValuesFromJson,
} from "./channel-forms";

describe("buildChannelSettingsJson", () => {
  it("builds openai settings from structured form values", () => {
    const result = buildChannelSettingsJson("openai", {
      base_url: "https://api.openai.com",
      user_agent: "",
    });
    expect(result).toEqual({ base_url: "https://api.openai.com" });
  });

  it("exposes the full codex oauth credential schema", () => {
    expect(credentialFieldsForChannel("codex").map((field) => field.key)).toEqual([
      "access_token",
      "refresh_token",
      "id_token",
      "user_email",
      "account_id",
      "expires_at_ms",
    ]);
  });

  it("exposes the full claudecode oauth credential schema", () => {
    expect(credentialFieldsForChannel("claudecode").map((field) => field.key)).toEqual([
      "access_token",
      "refresh_token",
      "expires_at_ms",
      "device_id",
      "account_uuid",
      "rate_limit_tier",
      "cookie",
      "user_email",
    ]);
  });

  it("exposes claudecode fingerprint settings instead of legacy user_agent", () => {
    const fieldKeys = settingsFieldsForChannel("claudecode").map((field) => field.key);

    expect(fieldKeys).toContain("fingerprint");
    expect(fieldKeys).not.toContain("user_agent");
    expect(defaultSettingsForChannel("claudecode")).not.toHaveProperty("user_agent");
  });

  it("roundtrips claudecode fingerprint settings as a structured object", () => {
    const values = settingsValuesFromJson("claudecode", {
      base_url: "https://api.anthropic.com",
      fingerprint: {
        cli_version: "9.8.7",
        user_type: "external",
        entrypoint: "cli",
      },
    });

    expect(values.fingerprint).toContain('"cli_version": "9.8.7"');

    expect(buildChannelSettingsJson("claudecode", values)).toMatchObject({
      base_url: "https://api.anthropic.com",
      fingerprint: {
        cli_version: "9.8.7",
        user_type: "external",
        entrypoint: "cli",
      },
    });
  });

  it("exposes the full geminicli oauth credential schema", () => {
    expect(credentialFieldsForChannel("geminicli").map((field) => field.key)).toEqual([
      "access_token",
      "refresh_token",
      "expires_at_ms",
      "project_id",
      "client_id",
      "client_secret",
      "user_email",
    ]);
  });

  it("exposes the full antigravity oauth credential schema", () => {
    expect(credentialFieldsForChannel("antigravity").map((field) => field.key)).toEqual([
      "access_token",
      "refresh_token",
      "expires_at_ms",
      "project_id",
      "client_id",
      "client_secret",
      "user_email",
    ]);
  });

  it("omits optional empty credential fields", () => {
    const result = buildCredentialJson("codex", {
      access_token: "token",
      refresh_token: "",
      id_token: "",
      user_email: "",
      account_id: "fdc791c5-acf2-4760-b8e7-4af508952763",
      expires_at_ms: "1776493967337",
    });

    expect(result).toEqual({
      access_token: "token",
      account_id: "fdc791c5-acf2-4760-b8e7-4af508952763",
      expires_at_ms: 1776493967337,
    });
  });
});

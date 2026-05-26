import { describe, expect, it } from "vitest";

import {
  buildChannelSettingsJson,
  buildCredentialJson,
  credentialFieldsForChannel,
  defaultSettingsForChannel,
  normalizeCredentialJson,
  parseCredentialImport,
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

  it("uses current antigravity defaults", () => {
    expect(defaultSettingsForChannel("antigravity")).toMatchObject({
      base_url: "https://cloudcode-pa.googleapis.com",
      user_agent: "antigravity/2.0.1 (Windows; AMD64)",
      oauth_authorize_url: "https://accounts.google.com/o/oauth2/v2/auth",
      oauth_token_url: "https://oauth2.googleapis.com/token",
      oauth_userinfo_url: "https://www.googleapis.com/oauth2/v1/userinfo?alt=json",
    });
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

  it("normalizes pasted claudecode cookie headers to the sessionKey value", () => {
    const result = buildCredentialJson("claudecode", {
      access_token: "",
      refresh_token: "",
      expires_at_ms: "",
      device_id: "",
      account_uuid: "",
      rate_limit_tier: "",
      cookie: "Cookie: other=1; sessionKey=sk-ant-sid01-example; theme=dark",
      user_email: "",
    });

    expect(result.cookie).toBe("sk-ant-sid01-example");
  });

  it("normalizes raw claudecode credential JSON before submit", () => {
    const result = normalizeCredentialJson("claudecode", {
      cookie: "sessionKey=sk-ant-sid01-raw; other=1",
    });

    expect(result).toEqual({ cookie: "sk-ant-sid01-raw" });
  });

  it("parses one raw API key per line for credential import", () => {
    expect(parseCredentialImport("openai", "sk-one\n\n  sk-two  ")).toEqual([
      { api_key: "sk-one" },
      { api_key: "sk-two" },
    ]);
  });

  it("parses multiple multiline JSON credential objects", () => {
    const input = `
{
  "api_key": "sk-one"
}
{
  "api_key": "sk-two",
  "label": "backup"
}
`;

    expect(parseCredentialImport("openai", input)).toEqual([
      { api_key: "sk-one" },
      { api_key: "sk-two", label: "backup" },
    ]);
  });

  it("parses JSON arrays with object and string credentials", () => {
    expect(parseCredentialImport("openai", `[{"api_key":"sk-one"}, "sk-two"]`)).toEqual([
      { api_key: "sk-one" },
      { api_key: "sk-two" },
    ]);
  });

  it("normalizes raw claudecode cookie lines during credential import", () => {
    expect(
      parseCredentialImport("claudecode", "Cookie: other=1; sessionKey=sk-ant-sid01-line"),
    ).toEqual([{ cookie: "sk-ant-sid01-line" }]);
  });

  it("rejects incomplete multiline JSON credentials", () => {
    expect(() => parseCredentialImport("openai", `{"api_key": "sk-one"`)).toThrow(
      /Invalid credential JSON/,
    );
  });
});

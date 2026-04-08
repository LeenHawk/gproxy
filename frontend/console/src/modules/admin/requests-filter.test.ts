import { describe, expect, it } from "vitest";

import {
  buildAdminUsageQuery,
  buildAdminUsageDeleteAllQuery,
  buildDownstreamRequestQuery,
  buildDownstreamDeleteAllQuery,
  buildUpstreamRequestQuery,
  buildUpstreamDeleteAllQuery,
} from "./requests-filter";

describe("buildDownstreamRequestQuery", () => {
  it("builds the downstream request filter payload", () => {
    expect(
      buildDownstreamRequestQuery({
        user_id: "7",
        user_key_id: "11",
        request_path_contains: "/v1/responses",
        limit: "50",
        include_body: true,
      }),
    ).toEqual({
      trace_id: "All",
      user_id: { Eq: 7 },
      user_key_id: { Eq: 11 },
      request_path_contains: "/v1/responses",
      limit: 50,
      include_body: true,
    });
  });
});

describe("buildUpstreamRequestQuery", () => {
  it("builds the upstream request filter payload", () => {
    expect(
      buildUpstreamRequestQuery({
        provider_id: "3",
        credential_id: "8",
        request_url_contains: "chat/completions",
        limit: "20",
        include_body: false,
      }),
    ).toEqual({
      trace_id: "All",
      provider_id: { Eq: 3 },
      credential_id: { Eq: 8 },
      request_url_contains: "chat/completions",
      limit: 20,
      include_body: false,
    });
  });
});

describe("delete-all request queries", () => {
  it("drops the downstream limit and request body when deleting all", () => {
    expect(
      buildDownstreamDeleteAllQuery({
        user_id: "7",
        user_key_id: "11",
        request_path_contains: "/v1/responses",
        limit: "50",
        include_body: true,
      }),
    ).toEqual({
      trace_id: "All",
      user_id: { Eq: 7 },
      user_key_id: { Eq: 11 },
      request_path_contains: "/v1/responses",
      include_body: false,
    });
  });

  it("drops the upstream limit and request body when deleting all", () => {
    expect(
      buildUpstreamDeleteAllQuery({
        provider_id: "3",
        credential_id: "8",
        request_url_contains: "chat/completions",
        limit: "20",
        include_body: true,
      }),
    ).toEqual({
      trace_id: "All",
      provider_id: { Eq: 3 },
      credential_id: { Eq: 8 },
      request_url_contains: "chat/completions",
      include_body: false,
    });
  });
});

describe("buildAdminUsageQuery", () => {
  it("includes the required scope fields for admin usages", () => {
    expect(
      buildAdminUsageQuery({
        provider_id: "3",
        credential_id: "8",
        channel: "geminicli",
        model: "gemini-2.5-pro",
        user_id: "7",
        user_key_id: "11",
        limit: "50",
      }),
    ).toEqual({
      provider_id: { Eq: 3 },
      credential_id: { Eq: 8 },
      channel: { Eq: "geminicli" },
      model: { Eq: "gemini-2.5-pro" },
      user_id: { Eq: 7 },
      user_key_id: { Eq: 11 },
      limit: 50,
    });
  });

  it("drops the usage limit when deleting all matching rows", () => {
    expect(
      buildAdminUsageDeleteAllQuery({
        provider_id: "3",
        credential_id: "8",
        channel: "geminicli",
        model: "gemini-2.5-pro",
        user_id: "7",
        user_key_id: "11",
        limit: "50",
      }),
    ).toEqual({
      provider_id: { Eq: 3 },
      credential_id: { Eq: 8 },
      channel: { Eq: "geminicli" },
      model: { Eq: "gemini-2.5-pro" },
      user_id: { Eq: 7 },
      user_key_id: { Eq: 11 },
    });
  });
});

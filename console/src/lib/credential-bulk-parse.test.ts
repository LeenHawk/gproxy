import { describe, expect, it } from "vitest";
import {
  bulkModeFor, dedupeItems, parseJsonInput, parseTokens,
} from "./credential-bulk-parse";

describe("bulkModeFor", () => {
  it("maps token families to tokens and JSON families to json", () => {
    expect(bulkModeFor("api_key")).toBe("tokens");
    expect(bulkModeFor("github_token")).toBe("tokens");
    expect(bulkModeFor("oauth_tokens")).toBe("json");
    expect(bulkModeFor("service_account")).toBe("json");
  });
});

describe("parseTokens", () => {
  it("parses lines with labels, comments and the family token field", () => {
    const { items } = parseTokens("github_token", "ghu_aaa,work\n# skip\n\nghu_bbb\n");
    expect(items).toHaveLength(2);
    expect(items[0]).toMatchObject({ label: "work", secret: { github_token: "ghu_aaa" } });
    expect(items[1]).toMatchObject({ label: null, secret: { github_token: "ghu_bbb" } });
  });
});

describe("parseJsonInput", () => {
  it("accepts an array, a single pretty object, and JSONL", () => {
    const arr = parseJsonInput("oauth_tokens", '[{"access_token":"a"},{"access_token":"b"}]', "f.json");
    expect(arr.items).toHaveLength(2);

    const single = parseJsonInput("oauth_tokens", '{\n  "access_token": "a"\n}', "f.json");
    expect(single.items).toHaveLength(1);

    const jsonl = parseJsonInput("oauth_tokens", '{"access_token":"a"}\nnot-json\n', "input");
    expect(jsonl.items).toHaveLength(1);
    expect(jsonl.errors).toEqual([{ source: "input#L2", code: "invalid_json" }]);
  });

  it("unwraps {label, secret} and validates service accounts", () => {
    const wrapped = parseJsonInput(
      "oauth_tokens",
      '{"label":"acc1","secret":{"access_token":"a"}}',
      "input",
    );
    expect(wrapped.items[0]).toMatchObject({ label: "acc1", secret: { access_token: "a" } });

    const sa = parseJsonInput("service_account", '{"client_email":"x@y","private_key":""}', "sa.json");
    expect(sa.items).toHaveLength(0);
    expect(sa.errors).toEqual([{ source: "sa.json", code: "invalid_secret" }]);
  });
});

describe("dedupeItems", () => {
  it("drops identical secrets and counts them", () => {
    const { items } = parseTokens("api_key", "sk-a\nsk-a,dup\nsk-b");
    const deduped = dedupeItems(items);
    expect(deduped.items).toHaveLength(2);
    expect(deduped.dupes).toBe(1);
  });
});

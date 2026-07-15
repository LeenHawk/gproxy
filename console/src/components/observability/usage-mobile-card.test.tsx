import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import type { Usage } from "@/api/usage";
import { UsageMobileCard } from "./usage-mobile-card";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

const usage: Usage = {
  id: 1,
  request_id: "req-1",
  at: 1_700_000_000,
  route_name: "default",
  provider_id: 7,
  credential_id: 8,
  org_id: null,
  team_id: null,
  user_id: 9,
  user_key_id: 10,
  operation: "messages",
  kind: "messages",
  model: "claude-test",
  input_tokens: 101,
  output_tokens: 202,
  cache_read_tokens: 606,
  cache_creation_5m_tokens: 303,
  cache_creation_30m_tokens: 404,
  cache_creation_1h_tokens: 505,
  cost: "0.12345",
  latency_ms: 707,
  usage_source: "upstream",
  ended: "complete",
};

describe("UsageMobileCard", () => {
  it("renders every usage metric shown by the desktop table", () => {
    const html = renderToStaticMarkup(
      <UsageMobileCard usage={usage} providerLabel="Provider seven" />,
    );

    expect(html).toContain("usage.columns.inputTokens");
    expect(html).toContain(">101<");
    expect(html).toContain("usage.columns.outputTokens");
    expect(html).toContain(">202<");
    expect(html).toContain("5m 303");
    expect(html).toContain("30m 404");
    expect(html).toContain("1h 505");
    expect(html).toContain("usage.columns.cacheRead");
    expect(html).toContain(">606<");
    expect(html).toContain("Provider seven");
  });
});

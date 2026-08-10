import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import type { UsageSummary } from "@/api/usage";
import { UsageSummaryBar } from "./usage-summary-bar";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

const summary: UsageSummary = {
  requests: 1_234,
  input_tokens: 2_345,
  output_tokens: 3_456,
  image_output_tokens: 8_901,
  cache_read_tokens: 4_567,
  cache_creation_5m_tokens: 5_678,
  cache_creation_30m_tokens: 6_789,
  cache_creation_1h_tokens: 7_890,
  cost: "1.234567",
};

describe("UsageSummaryBar", () => {
  it("renders every filtered total in a responsive summary grid", () => {
    const html = renderToStaticMarkup(<UsageSummaryBar summary={summary} />);

    expect(html).toContain("grid-cols-2");
    expect(html).toContain("lg:grid-cols-10");
    expect(html).toContain("usage.columns.imageOutputTokens");
    expect(html).toContain("1,234");
    expect(html).toContain("2,345");
    expect(html).toContain("5m");
    expect(html).toContain("30m");
    expect(html).toContain("1h");
    expect(html).toContain("7,890");
    expect(html).toContain("8,901");
    expect(html).toContain("66.1%");
    expect(html).toContain("$1.23457");
  });

  it("shows a dash when the cache hit rate has no input", () => {
    const html = renderToStaticMarkup(
      <UsageSummaryBar
        summary={{ ...summary, input_tokens: 0, cache_read_tokens: 0 }}
      />,
    );

    expect(html).toContain("usage.cacheHitRate");
    expect(html).toContain("—");
  });
});

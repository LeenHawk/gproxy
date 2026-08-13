import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { UsageCard } from "./usage-card";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

vi.mock("@tanstack/react-query", () => ({
  queryOptions: <T,>(options: T) => options,
  useMutation: () => ({ isPending: false, mutate: vi.fn() }),
  useQuery: () => ({
    data: undefined,
    error: undefined,
    isError: false,
    isFetched: true,
    isFetching: false,
    refetch: vi.fn(),
  }),
  useQueryClient: () => ({ invalidateQueries: vi.fn() }),
}));

vi.mock("@/components/providers/credential-usage-summary", () => ({
  CredentialUsageSummaryCard: () => <section>recorded-usage</section>,
  modelUsageBreakdown: vi.fn(),
}));

vi.mock("@/components/observability/credential-quota-history", () => ({
  CredentialQuotaHistory: () => <section>quota-history</section>,
}));

describe("UsageCard", () => {
  it("shows upstream quota before recorded usage when the channel supports it", () => {
    const html = renderToStaticMarkup(
      <UsageCard credentialId={7} supportsUpstreamUsage />,
    );

    expect(html.indexOf("usage.upstreamTitle")).toBeGreaterThanOrEqual(0);
    expect(html.indexOf("usage.upstreamTitle")).toBeLessThan(html.indexOf("recorded-usage"));
  });
});

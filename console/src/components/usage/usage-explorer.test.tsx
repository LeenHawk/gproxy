import { render, screen } from "@testing-library/react"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { describe, expect, it, vi } from "vitest"
import "@/i18n"
import { UsageExplorer } from "@/components/usage/usage-explorer"
import type { UsageRecordQueryDto } from "@/generated/UsageRecordQueryDto"

const query: UsageRecordQueryDto = {
  from: 1,
  to: 2,
  user_key_id: null,
  user_id: null,
  provider_id: null,
  credential_id: null,
  model: null,
  request_id: null,
  operation: null,
  usage_source: null,
  ended: null,
  page: 1,
  page_size: 10,
}

const props = {
  draft: query,
  onDraft: vi.fn(),
  onApply: vi.fn(),
  onReset: vi.fn(),
  page: {
    items: [{
      id: 1,
      request_id: "usage-switch-record",
      at: 1,
      provider_id: 1,
      credential_id: 1,
      user_id: null,
      user_key_id: null,
      operation: "generate_content",
      model: "gpt-test",
      input_tokens: 1,
      output_tokens: 1,
      cached_input_tokens: 0,
      metrics: {},
      dimensions: {},
      cost: "0",
      usage_source: "upstream",
      ended: "complete",
      latency_ms: 10,
    }],
    total: 1,
    page: 1,
    page_size: 10,
  },
  summary: {
    requests: 1,
    input_tokens: 1,
    output_tokens: 1,
    cached_input_tokens: 0,
    total_tokens: "2",
    cost: "0",
    metrics: {},
  },
  summaryError: false,
  pending: false,
  onPage: vi.fn(),
  onPageSize: vi.fn(),
  credentials: [],
  providers: [],
  users: [],
  keys: [],
}

describe("usage explorer views", () => {
  it("shows usage records and quota records as mutually exclusive views", () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    const explorer = (view: "records" | "quotas") => <QueryClientProvider client={client}>
      <UsageExplorer {...props} view={view}><div>quota-view</div></UsageExplorer>
    </QueryClientProvider>
    const { rerender } = render(explorer("records"))
    expect(screen.getAllByText("usage-switch-record")).not.toHaveLength(0)
    expect(screen.queryByText("quota-view")).not.toBeInTheDocument()

    rerender(explorer("quotas"))
    expect(screen.getByText("quota-view")).toBeInTheDocument()
    expect(screen.queryAllByText("usage-switch-record")).toHaveLength(0)
    expect(screen.queryByLabelText("User")).not.toBeInTheDocument()
  })
})

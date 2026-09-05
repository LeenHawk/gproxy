import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { render, screen, within } from "@testing-library/react"
import { expect, it, vi } from "vitest"
import type { UsageRecordDto } from "@/generated/UsageRecordDto"
import { UsageTable } from "@/components/usage/usage-table"

vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key, i18n: { language: "en" } }),
}))

it("shows time, total duration and average output TPS, including zero-duration records", () => {
  const record: UsageRecordDto = {
    id: 1, request_id: "usage-timing", at: 1788566400, provider_id: 1, credential_id: 1,
    user_id: null, user_key_id: null, operation: null, model: "test-model",
    input_tokens: 800, output_tokens: 100, cached_input_tokens: 0,
    metrics: {}, dimensions: {}, cost: "0.01", usage_source: "upstream", ended: "complete", latency_ms: 2000,
  }
  localStorage.clear()
  render(<QueryClientProvider client={new QueryClient()}>
    <UsageTable page={{ items: [record, { ...record, id: 2, output_tokens: 0 }, { ...record, id: 3, latency_ms: 0 }], page: 1, page_size: 10, total: 3 }}
      providers={[]} credentials={[]} users={[]} keys={[]} pending={false} onPage={vi.fn()} onPageSize={vi.fn()} />
  </QueryClientProvider>)

  const table = within(screen.getByRole("table"))
  const headers = table.getAllByRole("columnheader").map((header) => header.textContent)
  expect(headers.slice(0, 3)).toEqual(["usage.record.time", "usage.record.latency", "usage.record.tps"])
  const rows = table.getAllByRole("row").slice(1)
  const cells = rows.map((row) => within(row).getAllByRole("cell"))
  expect(cells[0][0]).toHaveTextContent("Sep")
  expect(cells[0][1]).toHaveTextContent("2000 ms")
  expect(cells.map((row) => row[2].textContent)).toEqual(["50", "0", "—"])
  expect(cells[0][2].firstChild).toHaveAttribute("title", "usage.record.tpsHint")
})

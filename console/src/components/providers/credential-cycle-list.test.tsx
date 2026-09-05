import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import { render, screen, within } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import "@/i18n"
import { CredentialCycleList } from "@/components/providers/credential-cycle-list"
import { WindowList } from "@/components/usage/window-list"
import { validDateRange } from "@/lib/date-range"

const cycle: CredentialQuotaCycleDto = {
  id: 1,
  version: 1,
  credential_id: 7,
  window_key: "five-hour",
  label: null,
  period_start: 100,
  period_end: 200,
  boundary_source: "upstream",
  boundary_confidence: "exact",
  status: "open",
  close_reason: null,
  last_observed_at: 150,
  upstream_used: "10",
  upstream_limit: "100",
  used_percent: "10",
  coverage: "full_period_lower_bound",
  metrics: {},
  models: [],
}

describe("CredentialCycleList", () => {
  it("renders only the latest observation for each upstream window", () => {
    render(
      <CredentialCycleList
        cycles={[
          cycle,
          { ...cycle, id: 2, last_observed_at: 175, upstream_used: "20", used_percent: "20" },
        ]}
        loading={false}
        error={false}
      />,
    )

    expect(screen.getAllByText("five-hour")).toHaveLength(1)
    expect(screen.getByText("20%")).toBeInTheDocument()
    expect(screen.queryByText("10%")).toBeNull()
  })

  it("uses the latest upstream label for historical scoped quota windows", () => {
    render(
      <WindowList
        cycles={[
          { ...cycle, window_key: "additional_secondary:bengalfox", label: "Codex bengalfox", last_observed_at: 140 },
          { ...cycle, id: 2, window_key: "additional_secondary:bengalfox", label: "GPT-5.3-Codex-Spark", last_observed_at: 150 },
        ]}
      />,
    )

    expect(screen.getAllByText(/GPT-5.3-Codex-Spark/)).toHaveLength(2)
    expect(screen.queryByText(/Bengalfox/)).toBeNull()
  })

  it("accepts only explicit start and end bounds in chronological order", () => {
    expect(validDateRange({ start: 100, end: 200 })).toBe(true)
    expect(validDateRange({ start: 200, end: 200 })).toBe(false)
    expect(validDateRange({ start: 300, end: 200 })).toBe(false)
  })

  it("shows local cycle usage and equivalent capacity without double-counting cache reads", () => {
    render(<WindowList cycles={[{ ...cycle, used_percent: "25", metrics: {
      input_tokens: "800", output_tokens: "200", cached_input_tokens: "600",
      cache_creation_5m_tokens: "100", cache_creation_30m_tokens: "200", cache_creation_1h_tokens: "300",
      cost: "2", requests: "4",
    } }]} />)

    expect(screen.getByText("1,600 tokens · $2.00 · 4 requests")).toBeInTheDocument()
    expect(screen.getByText("≈ 6,400 tokens · $8.00")).toBeInTheDocument()
  })

  it("falls back to upstream used / limit only when the percentage is absent", () => {
    const metrics = { input_tokens: "800", output_tokens: "200", cost: "2" }
    const { rerender } = render(<WindowList cycles={[{ ...cycle, used_percent: null, metrics }]} />)
    expect(screen.getByText("≈ 10,000 tokens · $20.00")).toBeInTheDocument()

    rerender(<WindowList cycles={[{ ...cycle, used_percent: "0", metrics }]} />)
    expect(screen.getByText("Insufficient data")).toBeInTheDocument()
    expect(screen.queryByText(/≈/)).toBeNull()
  })

  it("keeps missing local usage unknown and does not extrapolate zero usage", () => {
    const { rerender } = render(<WindowList cycles={[cycle]} />)
    expect(within(screen.getByText("Used this cycle (local)").parentElement!).getByRole("definition"))
      .toHaveTextContent("No local usage recorded")
    expect(screen.getByText("Insufficient data")).toBeInTheDocument()

    rerender(<WindowList cycles={[{ ...cycle, metrics: { input_tokens: "0", output_tokens: "0", cost: "0", requests: "0" } }]} />)
    expect(screen.getByText(/0 tokens .* 0 requests/)).toBeInTheDocument()
    expect(screen.getByText("Insufficient data")).toBeInTheDocument()
  })
})

import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import "@/i18n"
import { CredentialCycleList } from "@/components/providers/credential-cycle-list"
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

  it("accepts only explicit start and end bounds in chronological order", () => {
    expect(validDateRange({ start: 100, end: 200 })).toBe(true)
    expect(validDateRange({ start: 200, end: 200 })).toBe(false)
    expect(validDateRange({ start: 300, end: 200 })).toBe(false)
  })
})

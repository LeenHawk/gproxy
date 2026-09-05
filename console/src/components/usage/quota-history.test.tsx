import { render, screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { describe, expect, it } from "vitest"
import "@/i18n"
import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import type { CycleObservationDto } from "@/generated/CycleObservationDto"
import { QuotaHistory } from "./quota-history"
import { cyclePoints, remainingQuota, roundRange } from "./quota-history-data"

const sample = (at: number, percent: string | null, tokens: string | null = null): CycleObservationDto => ({
  observed_at_ms: at, used_percent: percent, upstream_used: null, upstream_limit: null, unit: null,
  estimate: tokens == null ? null : { tokens, cost: "20", reason: null, from_ms: 100000, to_ms: at },
})
const cycle: CredentialQuotaCycleDto = {
  id: 1, version: 1, credential_id: 7, window_key: "primary", label: null,
  period_start: 100, period_end: 200, accounting_start_ms: 100000, accounting_end_ms: 200000,
  boundary_source: "upstream", boundary_confidence: "exact", status: "closed", close_reason: "boundary_crossed",
  last_observed_at: 180, upstream_used: null, upstream_limit: null, used_percent: "50", coverage: "partial_lower_bound",
  metrics: {}, models: [], unit: null, local_boundary: false, estimate: null,
  observations: [sample(110000, "10", "1000"), sample(150000, "50", "3000"), sample(180000, "25", "800")],
}

describe("quota history", () => {
  it("keeps all valid values in each round's range, with the last value rather than the maximum marked", () => {
    expect(roundRange(cycle, "tokens")).toEqual({ at: 100000, value: 600, minimum: 600, maximum: 1500, range: [0, 900], count: 3, cycleId: 1, observedAt: 180000 })
    expect(roundRange(cycle, "percent")).toMatchObject({ value: 75, minimum: 50, maximum: 90, range: [25, 15], count: 3 })
    expect(roundRange({ ...cycle, observations: [sample(180000, "25", "800")] }, "tokens")).toMatchObject({ value: 600, range: [0, 0], count: 1 })
  })

  it("preserves missing samples as gaps and never extrapolates history or substitutes zero", () => {
    const missing = { ...cycle, observations: [sample(180000, "25", "800"), sample(150000, null), sample(110000, "10", "1000")] }
    expect(cyclePoints(missing, "tokens")).toEqual([{ at: 110000, value: 900 }, { at: 150000, value: null }, { at: 180000, value: 600 }])
    expect(roundRange({ ...cycle, observations: [] }, "percent")).toBeNull()
    expect(remainingQuota({ ...sample(100000, null), upstream_used: "20", upstream_limit: "80" }, "percent")).toBe(75)
    expect(remainingQuota(sample(100000, "0"), "tokens")).toBeNull()
    expect(remainingQuota(sample(100000, "100", "1000"), "tokens")).toBe(0)
    expect(remainingQuota({ ...sample(100000, "25", "1000"), estimate: { tokens: "1000", cost: "20", reason: "incomplete_usage", from_ms: null, to_ms: null } }, "tokens")).toBeNull()
  })

  it("renders both charts before details and applies round and channel selection to all three", async () => {
    const user = userEvent.setup()
    render(<QuotaHistory cycles={[cycle, { ...cycle, id: 2, accounting_start_ms: 200000 }]} providers={[]} credentials={[]} loading={false} error={false} />)
    const section = screen.getByRole("region", { name: "Upstream quota history" })
    const titles = within(section).getAllByText(/Remaining quota within each round|Remaining quota across rounds|Upstream credential quota cycles/)
    expect(titles.map((node) => node.textContent)).toEqual(["Remaining quota within each round", "Remaining quota across rounds", "Upstream credential quota cycles"])
    expect(screen.queryByText("Estimated total cycle capacity")).not.toBeInTheDocument()
    expect(screen.queryByText(/Estimated for the sampled model mix/)).not.toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "Rounds" }))
    await user.click(screen.getByRole("checkbox", { name: /#2$/ }))
    expect(screen.getByRole("button", { name: "Rounds" })).toHaveTextContent("1/2")
    await user.keyboard("{Escape}")
    await user.click(screen.getByRole("button", { name: "Channels" }))
    await user.click(screen.getByRole("button", { name: "Clear selection" }))
    await user.keyboard("{Escape}")
    expect(screen.getAllByText("Select channels and rounds to compare.")).toHaveLength(2)
    expect(screen.queryByText(/#1 · Period starts:/)).not.toBeInTheDocument()
  })
})

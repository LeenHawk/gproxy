import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import "@/i18n"
import type { QuotaProbeWindowDto } from "@/generated/QuotaProbeWindowDto"
import { CredentialCycleList } from "./credential-cycle-list"

const window: QuotaProbeWindowDto = {
  window_key: "3p-5h", label: null, used_percent: "25", period_end: 2000000000,
  upstream_used: null, upstream_limit: null, unit: null,
}

describe("Antigravity quota windows", () => {
  it("shows the independent five-hour window and its reset", () => {
    render(<CredentialCycleList cycles={[]} windows={[window]} loading={false} error={false} />)
    expect(screen.getByText("Claude / GPT · 5-hour quota")).toBeInTheDocument()
    expect(screen.getByText("25%")).toBeInTheDocument()
    expect(screen.getByText(/Resets /)).toBeInTheDocument()
  })
  it("does not imply a disabled five-hour window resets access before the weekly cap", () => {
    render(<CredentialCycleList cycles={[]} windows={[{ ...window, label: "antigravity_disabled", used_percent: null }]} loading={false} error={false} />)
    expect(screen.getByText("Claude / GPT · 5-hour quota (inactive)")).toBeInTheDocument()
    expect(screen.getByText(/Check the weekly quota for this model group/)).toBeInTheDocument()
    expect(screen.queryByText(/Resets /)).not.toBeInTheDocument()
    expect(screen.queryByText("0%")).not.toBeInTheDocument()
  })
})

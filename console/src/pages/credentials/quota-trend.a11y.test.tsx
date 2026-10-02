import { render, screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { expect, it, vi } from "vitest"
import "@/i18n"
import type { CredentialCycleDto } from "@/generated/sdk"
import QuotaTrend from "./quota-trend"

vi.mock("@/api/credentials", () => ({ credentialQuotaObservations: async () => ({
  items: [25, 50].map((used, index) => ({ id: `sample-${index}`, credentialId: "credential", scope: {}, snapshot: { id: "weekly", used_percent: String(used) }, observedAtMs: 1790812800000 + index * 3600000, startsAtMs: null, resetsAtMs: null, cycleId: "cycle", cycleCostUsd: "1" })),
  total: 2, offset: 0, limit: 500,
}) }))

it("provides the recorded values as a keyboard-reachable table", async () => {
  const cycle: CredentialCycleDto = { id: "cycle", credentialId: "credential", windowId: "weekly", dimensionId: null, scope: {}, startsAtMs: 1790812800000, endsAtMs: null, boundary: "observed", openedBy: "first_use", closedAtMs: null, costUsd: "1", sample: null, estimatedAllowanceUsd: null }
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const { container } = render(<QueryClientProvider client={client}><QuotaTrend id="credential" windowId="weekly" title="Weekly allowance" cycle={cycle} /></QueryClientProvider>)
  await screen.findByRole("group", { name: "Weekly allowance · Usage trend" })
  const summary = container.querySelector("summary")!
  await userEvent.click(summary)
  const table = screen.getByRole("table", { name: "Weekly allowance · Usage trend" })
  expect(within(table).getByText("25%")).toBeVisible()
  expect(within(table).getByText("50%")).toBeVisible()
  expect(screen.getByRole("group", { name: "Scrollable table" })).toHaveAttribute("tabindex", "0")
})

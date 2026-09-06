import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, expect, it, vi } from "vitest"
import type { QuotaDto } from "@/generated/QuotaDto"
import "@/i18n"
import { CredentialBudget } from "./credential-budget"

afterEach(() => vi.unstubAllGlobals())

it("saves a daily-only credential limit, reloads it, and shows exhaustion", async () => {
  let stored: QuotaDto | undefined
  const fetchMock = vi.fn<typeof fetch>().mockImplementation(async (path, init) => {
    if (init?.method === "POST" || init?.method === "PATCH") {
      stored = { id: 17, ...JSON.parse(String(init.body)) }
      return Response.json({ id: 17 })
    }
    if (String(path).includes("quota-windows")) {
      return Response.json(stored ? [{ id: 1, quota_id: 17, subject_kind: "credential", subject_id: 7,
        window_kind: "daily", window_start: 0, reset_at: 2_000_000_000, started: true,
        cost_used: "2.50", cost_limit: "2.50" }] : [])
    }
    return Response.json(stored ? [stored] : [])
  })
  vi.stubGlobal("fetch", fetchMock)
  const user = userEvent.setup()
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const view = render(<QueryClientProvider client={client}><CredentialBudget credentialId={7} /></QueryClientProvider>)
  await user.type(await screen.findByRole("spinbutton", { name: "Daily limit" }), "2.50")
  await user.click(screen.getByRole("button", { name: "Save" }))
  await waitFor(() => expect(stored).toMatchObject({ subject_kind: "credential", subject_id: 7,
    quota_total: null, quota_monthly: null, quota_weekly: null, quota_daily: "2.5", enabled: true }))
  expect(await screen.findByText("Limit reached")).toBeInTheDocument()
  expect(screen.getByRole("spinbutton", { name: "Daily limit" })).toHaveValue(2.5)
  await user.click(screen.getByRole("switch"))
  await user.click(screen.getByRole("button", { name: "Save" }))
  await waitFor(() => expect(stored?.enabled).toBe(false))
  await waitFor(() => expect(screen.queryByText("Limit reached")).not.toBeInTheDocument())
  view.unmount()
  client.clear()
})

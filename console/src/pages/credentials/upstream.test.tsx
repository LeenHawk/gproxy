import { fireEvent, render, screen, waitFor, within } from "@testing-library/react"
import { beforeEach, expect, it, vi } from "vitest"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import "@/i18n"
import { UpstreamQuota } from "./upstream"
import { credentialQuota, probeQuota, quotaResetCredits, resetUpstreamQuota } from "@/api/credentials"
import type { CredentialProviderDto } from "@/generated/app"
import type { QuotaSnapshotDto } from "@/generated/sdk"

vi.mock("@/api/credentials", () => ({ credentialQuota: vi.fn(), probeQuota: vi.fn(), quotaResetCredits: vi.fn(), resetUpstreamQuota: vi.fn() }))
const provider: CredentialProviderDto = { displayName: null, id: "p", name: "Codex", channel: "codex", enabled: true, loginModes: [], capabilities: { refresh: true, quotaQuery: true, quotaReset: true, services: true, websocket: true } }
const snapshot: QuotaSnapshotDto = { observedAtMs: 1_790_330_400_000, entries: [
  { id: "codex_primary", sourceId: "codex_primary", label: null, kind: "window", breakdown: null, subject: "account", modelScope: "all", balance: null, allowance: { used: "96", limit: "100", remaining: "4", usedPercent: "96", unlimited: null, unit: "percent", periodStartMs: 1_789_807_204_000, periodEndMs: 1_790_412_004_000, resetBehavior: "periodic" } },
  { id: "codex_credits", sourceId: "codex_credits", label: "credits", kind: "balance", breakdown: null, subject: "account", modelScope: "all", allowance: null, balance: { remaining: "0", unit: "credits" } },
] }
function mount(channel = "codex") {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  return render(<QueryClientProvider client={client}><UpstreamQuota id="c" provider={{ ...provider, channel }} /></QueryClientProvider>)
}
beforeEach(() => {
  vi.resetAllMocks()
  vi.mocked(credentialQuota).mockResolvedValue({ cycles: [], blocks: [] })
  vi.mocked(probeQuota).mockResolvedValue(snapshot)
  vi.mocked(quotaResetCredits).mockResolvedValue({ creditExpirationsMs: [], options: [], availableCount: 2, expiresAtMs: 1_800_000_000_000 })
  vi.mocked(resetUpstreamQuota).mockResolvedValue({ outcome: "reset", windowsReset: 1, reason: null })
})
it("renders the reported period and percent without exposing internal keys or epoch dates", async () => {
  mount()
  expect(await screen.findByRole("progressbar", { name: "7 days quota" })).toHaveAttribute("aria-valuenow", "96")
  expect(screen.getByText("96%")).toBeInTheDocument()
  expect(screen.queryByText("4%")).not.toBeInTheDocument()
  expect(screen.queryByText(/Remaining|Last observed/)).not.toBeInTheDocument()
  expect(screen.getByText("Extra credits")).toBeInTheDocument()
  expect(screen.queryByText(/codex_primary|percent|1970/)).not.toBeInTheDocument()
  expect(screen.getByTitle(/^Next reset:/)).toHaveAttribute("datetime", new Date(snapshot.entries[0].allowance!.periodEndMs!).toISOString())
  expect(resetUpstreamQuota).not.toHaveBeenCalled()
})
it("keeps reset-card queries independent from usage and does not hide usage on card failure", async () => {
  vi.mocked(quotaResetCredits).mockRejectedValue(new Error("Reset cards unavailable"))
  mount()
  await screen.findByText("Reset cards unavailable")
  expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "96")
  expect(screen.getByRole("button", { name: "Reset upstream quota" })).toBeDisabled()
  fireEvent.click(screen.getByRole("button", { name: "Refresh reset credits" }))
  await waitFor(() => expect(quotaResetCredits).toHaveBeenCalledTimes(2))
  expect(probeQuota).toHaveBeenCalledTimes(1)
  fireEvent.click(screen.getByRole("button", { name: "Query upstream quota" }))
  await waitFor(() => expect(probeQuota).toHaveBeenCalledTimes(2))
  expect(quotaResetCredits).toHaveBeenCalledTimes(2)
})
it("requires confirmation and refreshes usage and cards after a mocked reset", async () => {
  mount()
  const button = await screen.findByRole("button", { name: "Reset upstream quota" })
  await waitFor(() => expect(button).toBeEnabled())
  fireEvent.click(button)
  const dialog = screen.getByRole("alertdialog")
  expect(resetUpstreamQuota).not.toHaveBeenCalled()
  vi.mocked(quotaResetCredits).mockResolvedValue({ creditExpirationsMs: [], options: [], availableCount: 0, expiresAtMs: null })
  fireEvent.click(within(dialog).getByRole("button", { name: "Reset upstream quota" }))
  await waitFor(() => expect(resetUpstreamQuota).toHaveBeenCalledOnce())
  await waitFor(() => expect(quotaResetCredits).toHaveBeenCalledTimes(2))
  expect(probeQuota).toHaveBeenCalledTimes(2)
  await waitFor(() => expect(screen.getByRole("button", { name: "Reset upstream quota" })).toBeDisabled())
  expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument()
})

it("shows Claude eligibility separately and preserves the selected grant and request id on retry", async () => {
  vi.mocked(quotaResetCredits).mockResolvedValue({ creditExpirationsMs: [], availableCount: null, expiresAtMs: null, options: [
    { program: "juniper_tide", grantId: null, label: null, availableCount: null, expiresAtMs: null, nextAvailableAtMs: null, usable: false, ineligibleReason: "cli_version", clears: ["five_hour"] },
    { program: "cedar_ember", grantId: "gift-1", label: "Gift reset", availableCount: 2, expiresAtMs: 2_000_000_000_000, nextAvailableAtMs: null, usable: true, ineligibleReason: null, clears: ["five_hour", "seven_day"] },
  ] })
  vi.mocked(resetUpstreamQuota).mockRejectedValueOnce(new Error("Uncertain reply"))
    .mockResolvedValue({ outcome: "reset", windowsReset: 2, reason: null })
  mount("claudecode")
  expect(await screen.findByText(/client version is not eligible/)).toBeInTheDocument()
  expect(within(screen.getByRole("region", { name: "Weekly session reset" })).getByRole("button")).toBeDisabled()
  const gift = screen.getByRole("region", { name: "Gift reset" })
  await waitFor(() => expect(within(gift).getByRole("button")).toBeEnabled())
  fireEvent.click(within(gift).getByRole("button"))
  const dialog = screen.getByRole("alertdialog")
  expect(within(dialog).getByText("Resets: 5-hour quota, 7-day quota")).toBeInTheDocument()
  expect(resetUpstreamQuota).not.toHaveBeenCalled()
  fireEvent.click(within(dialog).getByRole("button", { name: "Reset upstream quota" }))
  await screen.findByText("Uncertain reply")
  const first = vi.mocked(resetUpstreamQuota).mock.calls[0]
  expect(first[1]).toMatchObject({ program: "cedar_ember", grantId: "gift-1", requestId: expect.any(String) })
  fireEvent.click(within(dialog).getByRole("button", { name: "Reset upstream quota" }))
  await waitFor(() => expect(resetUpstreamQuota).toHaveBeenCalledTimes(2))
  expect(vi.mocked(resetUpstreamQuota).mock.calls[1]).toEqual(first)
})

it("renders weekly composition separately and labels Fable as a weekly quota", async () => {
  const rows = [{ key: "claude_code", label: "Claude Code", percent: "70" }, { key: "chat", label: "Chats", percent: "20" }, { key: "cowork", label: "Cowork", percent: "10" }, { key: "other", label: "Other", percent: "0" }]
  vi.mocked(probeQuota).mockResolvedValue({ ...snapshot, entries: [
    { ...snapshot.entries[0], id: "seven_day", sourceId: "seven_day" },
    { ...snapshot.entries[0], id: "seven_day_breakdown", sourceId: "seven_day_breakdown", kind: "breakdown", allowance: null, breakdown: rows },
    { ...snapshot.entries[0], id: "weekly_model:fable", sourceId: "weekly_model:fable", label: "Fable" },
  ] })
  mount("claudecode")
  const bar = await screen.findByRole("img", { name: /Weekly usage breakdown: Claude Code 70%/ })
  expect(bar.children).toHaveLength(3)
  expect(screen.getAllByRole("progressbar")).toHaveLength(2)
  expect(screen.getByRole("heading", { name: "Fable · 7-day quota" })).toBeInTheDocument()
  expect(screen.queryByText("breakdown · 7-day quota")).not.toBeInTheDocument()
})

it("hides the legacy phantom breakdown window when falling back to saved observations", async () => {
  vi.mocked(probeQuota).mockRejectedValue(new Error("Offline"))
  vi.mocked(credentialQuota).mockResolvedValue({ blocks: [], cycles: [{
    id: "old-breakdown", credentialId: "c", scope: "all", snapshot: { id: "seven_day_breakdown", kind: "window", limit: "100", unit: "percent" },
    observedAtMs: 1_790_330_400_000, startsAtMs: null, resetsAtMs: null,
  }] })
  mount("claudecode")
  await screen.findByText("Offline")
  expect(screen.queryByText(/breakdown ·|— \/ 100/)).not.toBeInTheDocument()
})

it("shows a separate expiry for each available reset card", async () => {
  const expirations = [Date.parse("2026-10-04T02:24:54Z"), Date.parse("2026-10-05T04:19:45Z"), Date.parse("2026-10-22T20:44:53Z")]
  vi.mocked(quotaResetCredits).mockResolvedValue({ options: [], availableCount: 3, expiresAtMs: expirations[0], creditExpirationsMs: expirations })
  mount()
  await screen.findByText("Card 3")
  for (const [index, expiry] of expirations.entries()) {
    const card = screen.getByText(`Card ${index + 1}`).parentElement!
    expect(card.querySelector("time")).toHaveAttribute("datetime", new Date(expiry).toISOString())
  }
  expect(resetUpstreamQuota).not.toHaveBeenCalled()
})

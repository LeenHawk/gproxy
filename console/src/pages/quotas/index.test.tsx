import { fireEvent, render, screen, waitFor, within } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import "@/i18n"
import { QuotasPanel } from "./index"
import { quotas, budgetStatus, resetQuota } from "@/api/quotas"
import { credentialLimits } from "@/api/credentials"
import type { QuotaDto } from "@/generated/sdk"

const scope = vi.hoisted(() => ({ kind: "instance" }))
vi.mock("@/capability/session", () => ({ useConsoleContext: () => ({ scope, has: () => true }) }))
vi.mock("@/api/quotas", () => ({ quotas: { list: vi.fn(), create: vi.fn(), update: vi.fn(), remove: vi.fn() }, budgetStatus: vi.fn(), resetQuota: vi.fn() }))
vi.mock("@/api/credentials", () => ({ credentialLimits: vi.fn() }))
const parent: QuotaDto = { id: "default", ownerKind: "provider", ownerId: "p", windowKey: "Daily budget", metric: "cost", unit: "USD", limitValue: "100", period: "1d", periodSeconds: null, anchorAtMs: null, modelPattern: null, enabled: true }
let own: QuotaDto[]
function mount(ownerKind = "credential", ownerId = "c") {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  return render(<QueryClientProvider client={client}><QuotasPanel ownerKind={ownerKind} ownerId={ownerId} providerId="p" /></QueryClientProvider>)
}
beforeEach(() => {
  vi.resetAllMocks(); scope.kind = "instance"; own = []
  vi.mocked(quotas.list).mockImplementation(async filter => { const rows = filter.ownerKind === "provider" ? [parent] : own; return { items: [...rows], total: rows.length, offset: 0, limit: 500 } })
  vi.mocked(credentialLimits).mockImplementation(async () => {
    const rule = own.find(row => row.enabled) ?? parent
    return [{ quotaId: rule.id, ownerKind: rule.ownerKind, ownerId: rule.ownerId, windowKey: rule.windowKey, metric: rule.metric, period: rule.period, modelPattern: null, unit: "USD", limit: rule.limitValue, used: "7.5", windowStartMs: 1, windowEndMs: 2_000_000_000_000 }]
  })
  vi.mocked(budgetStatus).mockResolvedValue([])
  vi.mocked(quotas.create).mockImplementation(async body => { const row: QuotaDto = { ...parent, ...body, id: "override", windowKey: body.windowKey ?? parent.windowKey, enabled: body.enabled ?? true }; own = [row]; return row })
  vi.mocked(quotas.update).mockImplementation(async (id, body) => { const original = own.find(row => row.id === id)!; const row: QuotaDto = { ...original, enabled: body.enabled ?? original.enabled, limitValue: body.limitValue ?? original.limitValue }; own = own.map(item => item.id === id ? row : item); return row })
  vi.mocked(quotas.remove).mockImplementation(async () => { own = [] })
})
describe("contextual limits", () => {
  it("overrides and restores inherited limits inline without resetting other credentials", async () => {
    mount()
    await screen.findByText("Inherited from provider")
    expect(screen.queryByRole("button", { name: "Reset usage" })).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole("button", { name: "Set for this credential" }))
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
    expect(screen.getByLabelText("Rule name")).toHaveAttribute("readonly")
    fireEvent.change(screen.getByLabelText("Cost limit (USD)"), { target: { value: "25.50" } })
    fireEvent.click(screen.getByRole("button", { name: "Save" }))
    await screen.findByText("Credential override")
    expect(quotas.create).toHaveBeenCalledWith(expect.objectContaining({ ownerKind: "credential", ownerId: "c", windowKey: parent.windowKey, limitValue: "25.50", unit: "USD" }))
    fireEvent.click(screen.getByRole("button", { name: "Restore inheritance" }))
    fireEvent.click(within(screen.getByRole("alert")).getByRole("button", { name: "Restore inheritance" }))
    await screen.findByText("Inherited from provider")
    expect(quotas.remove).toHaveBeenCalledWith("override")
    expect(resetQuota).not.toHaveBeenCalled()
  })
  it("reuses a disabled override instead of creating a second rule with the same name", async () => {
    own = [{ ...parent, id: "disabled", ownerKind: "credential", ownerId: "c", limitValue: "20", enabled: false }]
    mount()
    fireEvent.click(await screen.findByRole("button", { name: "Set for this credential" }))
    fireEvent.change(screen.getByLabelText("Cost limit (USD)"), { target: { value: "30" } })
    fireEvent.click(screen.getByRole("button", { name: "Save" }))
    await waitFor(() => expect(quotas.update).toHaveBeenCalledWith("disabled", expect.objectContaining({ enabled: true, limitValue: "30" })))
    expect(quotas.create).not.toHaveBeenCalled()
  })

  it("shows provider defaults as per-credential limits without a fictitious total usage", async () => {
    mount("provider", "p")
    await screen.findByText(parent.windowKey)
    expect(screen.queryByText(/Each rule limits each credential separately/)).not.toBeInTheDocument()
    expect(screen.queryByText("Used", { exact: true })).not.toBeInTheDocument()
    expect(credentialLimits).not.toHaveBeenCalled()
    expect(budgetStatus).not.toHaveBeenCalled()
  })
  it("lets tenant administrators manage their credential's limits without requesting operator configuration", async () => {
    scope.kind = "team"
    mount()
    await screen.findByText("Inherited from provider")
    expect(screen.queryByText(/instance administrator can change/)).not.toBeInTheDocument()
    expect(quotas.list).not.toHaveBeenCalledWith(expect.objectContaining({ ownerKind: "provider" }))
    expect(screen.getByRole("button", { name: "Add limit" })).toBeInTheDocument()
    fireEvent.click(screen.getByRole("button", { name: "Set for this credential" }))
    fireEvent.change(screen.getByLabelText("Cost limit (USD)"), { target: { value: "12" } })
    fireEvent.click(screen.getByRole("button", { name: "Save" }))
    await screen.findByText("Credential override")
    expect(quotas.create).toHaveBeenCalledWith(expect.objectContaining({ ownerKind: "credential", ownerId: "c", limitValue: "12" }))
  })
  it("keeps provider limits read-only outside the instance scope", async () => {
    scope.kind = "organization"
    mount("provider", "p")
    await screen.findByText(parent.windowKey)
    expect(screen.getByText(/instance administrator can change/)).toBeInTheDocument()
    expect(screen.queryByRole("button", { name: "Add limit" })).not.toBeInTheDocument()
  })
  it("retains inline edits when a budget write fails", async () => {
    vi.mocked(quotas.create).mockRejectedValue(new Error("Budget refused"))
    mount("org", "o")
    fireEvent.click(await screen.findByRole("button", { name: "Add budget" }))
    fireEvent.change(screen.getByLabelText("Cost limit (USD)"), { target: { value: "80" } })
    fireEvent.click(screen.getByRole("button", { name: "Save" }))
    await screen.findByText("Budget refused")
    await waitFor(() => expect(screen.getByRole("button", { name: "Save" })).toBeEnabled())
    expect(screen.getByLabelText("Cost limit (USD)")).toHaveValue("80")
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
  })
})

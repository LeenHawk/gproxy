import type { CredentialDto } from "@/generated/CredentialDto"
import type { QuotaSnapshot } from "@/generated/QuotaSnapshot"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, describe, expect, it, vi } from "vitest"
import "@/i18n"
import { CredentialCard } from "@/components/providers/credential-card"
import { CredentialList } from "@/components/providers/credential-list"
import { TooltipProvider } from "@/components/ui/tooltip"

const credential: CredentialDto = {
  id: 7, provider_id: 3, label: "New credential", kind: "oauth",
  quota_capabilities: { probe: true, reset: false }, version: 1, enabled: true, weight: 100,
  rpm_limit: null, tpm_limit: null, proxy_url: null, tls_fingerprint: null,
  invalid_tls_fingerprint: null, tls_fingerprint_error: null, health: "unknown",
  health_observed_at: null, health_response_status: null, health_detail: null, model_health: [],
}

function snapshot(observed: number | null = Date.now()): QuotaSnapshot {
  return {
    sources: [{ capability: { id: "balance", label: "Account balance", kinds: ["balance"], mode: "probe", support: "ready", reason: null, automatic: true }, attempted_at_ms: observed, observed_at_ms: observed, error: null, reset_credits: null }],
    entries: observed == null ? [] : [{ id: "balance:CNY", source_id: "balance", label: "CNY balance", subject: "account", model_scope: { kind: "all" }, observed_at_ms: observed,
      value: { kind: "balance", remaining: "110.00", unit: "CNY", availability: "available", components: [{ kind: "granted", amount: "10.00" }, { kind: "topped_up", amount: "100.00" }] } }],
  }
}
const response = (body: unknown) => new Response(JSON.stringify(body), { status: 200, headers: { "content-type": "application/json" } })
const probed = (value: QuotaSnapshot) => ({ snapshot: value, windows: [], cycles: [], local_error: false, reset_credits: null, raw: "{}" })
function view(value = credential, client = new QueryClient({ defaultOptions: { queries: { retry: false } } })) {
  return <QueryClientProvider client={client}><TooltipProvider><CredentialCard credential={value} cycles={[]} cyclesLoading={false} cyclesError={false} /></TooltipProvider></QueryClientProvider>
}

describe("CredentialCard", () => {
  afterEach(() => vi.unstubAllGlobals())

  it("loads a saved snapshot before probing stale data and reuses fresh results on reopen", async () => {
    let resolveProbe!: (response: Response) => void
    const fetchMock = vi.fn<typeof fetch>()
      .mockResolvedValueOnce(response(snapshot(null)))
      .mockImplementationOnce(() => new Promise((resolve) => { resolveProbe = resolve }))
    vi.stubGlobal("fetch", fetchMock)
    const component = view()
    const mounted = render(component)
    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2))
    expect(fetchMock.mock.calls.map(([path]) => path)).toEqual(["/admin/api/credentials/7/quota", "/admin/api/credentials/7/quota-probe"])
    expect(screen.getByRole("button", { name: "Fetching…" })).toBeDisabled()
    resolveProbe(response(probed(snapshot())))
    await screen.findByText("110.00 CNY")
    mounted.rerender(<></>)
    mounted.rerender(component)
    expect(screen.getByText("110.00 CNY")).toBeInTheDocument()
    expect(fetchMock).toHaveBeenCalledTimes(2)
  })

  it("preserves an API Key's saved balance when refresh fails and reports per-source errors", async () => {
    const fresh = snapshot()
    const failed = snapshot()
    failed.sources[0].error = { code: "unauthorized", message: "Upstream returned 401" }
    const fetchMock = vi.fn<typeof fetch>().mockResolvedValueOnce(response(fresh)).mockResolvedValueOnce(response(probed(failed)))
    vi.stubGlobal("fetch", fetchMock)
    render(view({ ...credential, kind: "api_key", quota_capabilities: null }))
    await screen.findByText("110.00 CNY")
    expect(fetchMock).toHaveBeenCalledOnce()
    await userEvent.setup().click(screen.getByRole("button", { name: "Refresh" }))
    expect(await screen.findByText("Refresh failed; showing the last successful result.")).toBeInTheDocument()
    expect(screen.getByText("Upstream returned 401")).toBeInTheDocument()
    expect(screen.getByText("110.00 CNY")).toBeInTheDocument()
    expect(screen.queryByText("Upstream reports insufficient balance")).not.toBeInTheDocument()
    expect(fetchMock.mock.calls[1]?.[0]).toBe("/admin/api/credentials/7/quota-probe?force=true")
  })

  it("keeps unsupported and response-observed quota visible without active probing", async () => {
    const value = snapshot(null)
    value.sources[0].capability = { ...value.sources[0].capability, support: "unsupported", mode: "unavailable", reason: "No documented endpoint" }
    value.sources.push({ ...value.sources[0], capability: { ...value.sources[0].capability, id: "headers", label: "Request limits", mode: "response", support: "ready", reason: null } })
    const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(response(value))
    vi.stubGlobal("fetch", fetchMock)
    render(view({ ...credential, kind: "api_key", quota_capabilities: null }))
    expect(await screen.findByText("Quota queries are not supported yet.")).toBeInTheDocument()
    expect(screen.getByText("Updates with requests")).toBeInTheDocument()
    expect(screen.getByText("No quota observed yet.")).toBeInTheDocument()
    expect(screen.queryByRole("button", { name: "Refresh" })).not.toBeInTheDocument()
    expect(fetchMock).toHaveBeenCalledOnce()
  })

  it("separates unknown balances, unlimited budgets, rate limits and dated usage reports", async () => {
    const value = snapshot()
    const base = value.entries[0]
    value.entries = [
      { ...base, value: { kind: "balance", remaining: null, unit: "CNY", availability: "unknown", components: [] } },
      { ...base, id: "budget", label: "Key budget", subject: "key", value: { kind: "budget", used: "12", remaining: null, limit: null, unlimited: true, used_percent: null, unit: "USD", period_start: null, period_end: null } },
      { ...base, id: "rate", label: "Request rate", value: { kind: "rate_limit", used: "10", remaining: "90", limit: "100", unlimited: false, used_percent: "10", unit: "requests", period_start: null, period_end: 2_000_000_000 } },
      { ...base, id: "report", label: "Monthly usage", value: { kind: "usage_report", used: "19.123456", unit: "USD", period_start: 1_999_900_000, period_end: 2_000_000_000 } },
      { ...base, id: "empty", label: "USD balance", value: { kind: "balance", remaining: "0", unit: "USD", availability: "unavailable", components: [] } },
    ]
    vi.stubGlobal("fetch", vi.fn<typeof fetch>().mockResolvedValue(response(value)))
    render(view())
    expect(await screen.findByText("Availability unknown")).toBeInTheDocument()
    expect(screen.getByText("Unlimited")).toBeInTheDocument()
    expect(screen.getByText("90 requests")).toBeInTheDocument()
    expect(screen.getByText("19.123456 USD")).toBeInTheDocument()
    expect(screen.getByText(/Usage from/)).toBeInTheDocument()
    expect(screen.getByText("Upstream reports insufficient balance")).toBeInTheDocument()
    expect(screen.queryByText("0 CNY")).not.toBeInTheDocument()
  })

  it("marks old snapshots and only probes manual sources after Refresh", async () => {
    const old = snapshot(Date.now() - 601_000)
    old.sources[0].capability.automatic = false
    const fetchMock = vi.fn<typeof fetch>().mockResolvedValueOnce(response(old)).mockResolvedValueOnce(response(probed(snapshot())))
    vi.stubGlobal("fetch", fetchMock)
    render(view())
    expect(await screen.findByText("Snapshot older than 10 minutes")).toBeInTheDocument()
    expect(screen.getByText("110.00 CNY")).toBeInTheDocument()
    expect(fetchMock).toHaveBeenCalledOnce()
    await userEvent.setup().click(screen.getByRole("button", { name: "Refresh" }))
    await waitFor(() => expect(screen.queryByText("Snapshot older than 10 minutes")).not.toBeInTheDocument())
    expect(fetchMock.mock.calls[1]?.[0]).toBe("/admin/api/credentials/7/quota-probe?force=true")
  })

  it("keeps persisted reset credits visible and refreshes their count after redemption", async () => {
    const initial = snapshot()
    initial.sources[0].reset_credits = { available_count: 2, expires_at: 2_000_000_000 }
    const redeemed = snapshot()
    redeemed.sources[0].reset_credits = { available_count: 0, expires_at: null }
    const fetchMock = vi.fn<typeof fetch>()
      .mockResolvedValueOnce(response(initial))
      .mockResolvedValueOnce(response({ outcome: "reset", windows_reset: 1 }))
      .mockResolvedValueOnce(response(probed(redeemed)))
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    render(view({ ...credential, quota_capabilities: { probe: true, reset: true } }))
    const credits = screen.getByRole("region", { name: "Reset credits" })
    expect(within(credits).getByText("—")).toBeInTheDocument()
    expect(await within(credits).findByText("2")).toBeInTheDocument()
    expect(within(credits).getByText(/Expires/)).toBeInTheDocument()
    await user.click(within(credits).getByRole("button", { name: "Consume reset credit" }))
    expect(fetchMock).toHaveBeenCalledOnce()
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Consume credit" }))
    expect(await within(credits).findByText("0")).toBeInTheDocument()
    expect(fetchMock.mock.calls[1]?.[0]).toBe("/admin/api/credentials/7/quota-reset")
    expect(within(credits).getByRole("button", { name: "Consume reset credit" })).toBeDisabled()
  })

  it("retains expandable quota details for ordinary API Keys", async () => {
    const fetchMock = vi.fn<typeof fetch>().mockImplementation(async (path) => response(String(path).endsWith("/quota") ? snapshot() : []))
    vi.stubGlobal("fetch", fetchMock)
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    render(<QueryClientProvider client={client}><TooltipProvider>
      <CredentialList providerId={3} presets={[]} credentials={[{ ...credential, kind: "api_key", quota_capabilities: null }]} cyclesByCredential={new Map()}
        credentialsLoading={false} credentialsError={false} cyclesLoading={false} cyclesError={false}
        savingCredentialId={null} onSave={vi.fn()} />
    </TooltipProvider></QueryClientProvider>)
    expect(fetchMock).not.toHaveBeenCalled()
    await userEvent.setup().click(screen.getByRole("row", { name: /New credential/ }))
    expect(await within(screen.getByRole("table")).findByText("110.00 CNY")).toBeInTheDocument()
    expect(fetchMock.mock.calls.map(([path]) => path)).toContain("/admin/api/credentials/7/quota")
    expect(fetchMock.mock.calls.map(([path]) => path)).not.toContain("/admin/api/credentials/7/quota-probe")
  })
})

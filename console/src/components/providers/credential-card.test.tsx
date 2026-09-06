import type { CredentialDto } from "@/generated/CredentialDto"
import type { QuotaResetCreditsDto } from "@/generated/QuotaResetCreditsDto"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, describe, expect, it, vi } from "vitest"
import "@/i18n"
import { CredentialCard } from "@/components/providers/credential-card"
import { CredentialList } from "@/components/providers/credential-list"
import { TooltipProvider } from "@/components/ui/tooltip"

const credential: CredentialDto = {
  id: 7,
  provider_id: 3,
  label: "New credential",
  kind: "oauth",
  quota_capabilities: { probe: true, reset: false },
  version: 1,
  enabled: true,
  weight: 100,
  rpm_limit: null,
  tpm_limit: null,
  proxy_url: null,
  tls_fingerprint: null,
  invalid_tls_fingerprint: null,
  tls_fingerprint_error: null,
  health: "unknown",
  health_observed_at: null,
  health_response_status: null,
  health_detail: null,
  model_health: [],
}

describe("CredentialCard", () => {
  afterEach(() => vi.unstubAllGlobals())

  it("loads usage on first open and reuses fresh data on reopen", async () => {
    let resolveProbe!: (response: Response) => void
    const fetchMock = vi.fn<typeof fetch>().mockImplementation(() => new Promise((resolve) => { resolveProbe = resolve }))
    vi.stubGlobal("fetch", fetchMock)
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    const view = (
      <QueryClientProvider client={client}>
        <TooltipProvider>
          <CredentialCard
            credential={credential}
            cycles={[]}
            cyclesLoading={false}
            cyclesError={false}
          />
        </TooltipProvider>
      </QueryClientProvider>
    )
    const mounted = render(view)

    await waitFor(() => expect(fetchMock).toHaveBeenCalledOnce())
    expect(fetchMock.mock.calls[0]?.[0]).toBe("/admin/api/credentials/7/quota-probe")
    expect(screen.getByText("Loading…")).toBeInTheDocument()

    resolveProbe(new Response(JSON.stringify({ windows: [], cycles: [], local_error: false, reset_credits: null, raw: "{}" }), {
      status: 200,
      headers: { "content-type": "application/json" },
    }))
    await waitFor(() => expect(screen.getByText("The usage endpoint reported no quota windows.")).toBeInTheDocument())
    mounted.rerender(<></>)
    mounted.rerender(view)
    expect(screen.getByText("The usage endpoint reported no quota windows.")).toBeInTheDocument()
    expect(fetchMock).toHaveBeenCalledOnce()
  })

  it("does not render or probe ordinary API credentials", () => {
    const fetchMock = vi.fn<typeof fetch>()
    vi.stubGlobal("fetch", fetchMock)
    const { container } = render(<CredentialCard credential={{ ...credential, quota_capabilities: null }} cycles={[]} cyclesLoading={false} cyclesError={false} />)
    expect(container).toBeEmptyDOMElement()
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it("keeps reset credits visible when unknown or empty and refreshes their count after redemption", async () => {
    const result = (reset_credits: QuotaResetCreditsDto | null) => ({ windows: [], cycles: [], local_error: false, reset_credits, raw: "{}" })
    const response = (body: unknown) => new Response(JSON.stringify(body), { status: 200, headers: { "content-type": "application/json" } })
    const fetchMock = vi.fn<typeof fetch>()
      .mockResolvedValueOnce(response(result(null)))
      .mockResolvedValueOnce(response(result({ available_count: 2, expires_at: 2_000_000_000 })))
      .mockResolvedValueOnce(response({ outcome: "reset", windows_reset: 1 }))
      .mockResolvedValueOnce(response(result({ available_count: 0, expires_at: null })))
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    const view = (reset: boolean) => <QueryClientProvider client={client}><TooltipProvider>
      <CredentialCard credential={{ ...credential, quota_capabilities: { probe: true, reset } }} cycles={[]} cyclesLoading={false} cyclesError={false} />
    </TooltipProvider></QueryClientProvider>
    const mounted = render(view(true))
    const credits = screen.getByRole("region", { name: "Reset credits" })
    expect(within(credits).getByText("—")).toBeInTheDocument()
    expect(within(credits).getByRole("button", { name: "Consume reset credit" })).toBeDisabled()
    await screen.findByText("The usage endpoint reported no quota windows.")
    await user.click(screen.getByRole("button", { name: "Refresh" }))
    expect(await within(credits).findByText("2")).toBeInTheDocument()
    expect(within(credits).getByText(/Expires/)).toBeInTheDocument()
    await user.click(within(credits).getByRole("button", { name: "Consume reset credit" }))
    const dialog = screen.getByRole("alertdialog")
    expect(fetchMock).toHaveBeenCalledTimes(2)
    await user.click(within(dialog).getByRole("button", { name: "Consume credit" }))
    expect(await within(credits).findByText("0")).toBeInTheDocument()
    expect(fetchMock.mock.calls[2]?.[0]).toBe("/admin/api/credentials/7/quota-reset")
    expect(within(credits).getByRole("button", { name: "Consume reset credit" })).toBeDisabled()
    mounted.rerender(view(false))
    expect(within(credits).getByText("0")).toBeInTheDocument()
    expect(within(credits).queryByRole("button")).not.toBeInTheDocument()
  })

  it("omits the quota column without removing expandable quota details", async () => {
    const fetchMock = vi.fn<typeof fetch>().mockImplementation(async (path) => new Response(JSON.stringify(
      String(path).endsWith("/quota-probe") ? { windows: [], cycles: [], local_error: false, reset_credits: null, raw: "{}" } : [],
    ), { status: 200, headers: { "content-type": "application/json" } }))
    vi.stubGlobal("fetch", fetchMock)
    const user = userEvent.setup()
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    render(
      <QueryClientProvider client={client}>
        <TooltipProvider>
          <CredentialList providerId={3} presets={[]} credentials={[credential]} cyclesByCredential={new Map()}
            credentialsLoading={false} credentialsError={false} cyclesLoading={false} cyclesError={false}
            savingCredentialId={null} onSave={vi.fn()} />
        </TooltipProvider>
      </QueryClientProvider>,
    )

    expect(screen.queryByRole("columnheader", { name: "Upstream credential quota windows" })).not.toBeInTheDocument()
    expect(fetchMock).not.toHaveBeenCalled()
    await user.click(screen.getByRole("row", { name: /New credential/ }))
    expect(await within(screen.getByRole("table")).findByText("The usage endpoint reported no quota windows.")).toBeInTheDocument()
    expect(fetchMock).toHaveBeenCalledTimes(3)
    expect(fetchMock.mock.calls.map(([path]) => path)).toContain("/admin/api/credentials/7/quota-probe")
  })
})

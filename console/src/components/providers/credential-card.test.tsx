import type { CredentialDto } from "@/generated/CredentialDto"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { render, screen, waitFor } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"
import "@/i18n"
import { CredentialCard } from "@/components/providers/credential-card"
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
})

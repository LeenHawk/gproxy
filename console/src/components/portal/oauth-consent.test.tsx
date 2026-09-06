import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { beforeEach, expect, test, vi } from "vitest"
import "@/i18n"
import { OAuthConsent } from "@/components/portal/oauth-consent"
import { OAuthSessions } from "@/components/portal/oauth-sessions"
import { decideOAuthDevice, oauthDeviceConsent } from "@/api/oauth"
import { portalOAuthSessions, revokePortalOAuthSession } from "@/api/portal"

vi.mock("@/api/oauth", () => ({ oauthDeviceConsent: vi.fn(), decideOAuthDevice: vi.fn() }))
vi.mock("@/api/portal", () => ({ portalOAuthSessions: vi.fn(), revokePortalOAuthSession: vi.fn() }))

beforeEach(() => {
  vi.resetAllMocks()
  vi.mocked(oauthDeviceConsent).mockResolvedValue({ client_id: "pi-gproxy", client_name: "GPROXY for Pi", user_name: "Demo account", scope: "gproxy", user_code: "ABCD-EFGH" })
  vi.mocked(decideOAuthDevice).mockResolvedValue(undefined)
})

function wrapper({ children }: { children: React.ReactNode }) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>
}

test("device consent displays account and application before allowing or denying", async () => {
  const user = userEvent.setup()
  render(<OAuthConsent authorization={null} deviceCode="ABCD-EFGH" />, { wrapper })
  expect((await screen.findAllByText("GPROXY for Pi"))[0]).toBeInTheDocument()
  expect(screen.getByText("Demo account")).toBeInTheDocument()
  expect(screen.getByText("pi-gproxy")).toBeInTheDocument()
  expect(decideOAuthDevice).not.toHaveBeenCalled()
  await user.click(screen.getByRole("button", { name: "Deny" }))
  expect(await screen.findByText("Authorization denied")).toBeInTheDocument()
  expect(decideOAuthDevice).toHaveBeenCalledWith({ user_code: "ABCD-EFGH", approved: false })
})

test("revocation requires confirmation and refreshes the list and summary", async () => {
  const user = userEvent.setup()
  vi.mocked(portalOAuthSessions).mockResolvedValue({ total_logins: 2, active_sessions: 1, total: 1, sessions: [{
    id: 7, client_id: "pi-gproxy", client_name: "GPROXY for Pi", logged_in_at: 100,
    last_refreshed_at: 110, refresh_count: 3, refresh_expires_at: 10_000, revoked_at: null, active: true,
  }] })
  vi.mocked(revokePortalOAuthSession).mockResolvedValue(undefined)
  render(<OAuthSessions />, { wrapper })
  expect((await screen.findAllByText("GPROXY for Pi"))[0]).toBeInTheDocument()
  expect(screen.getByText("Total logins")).toBeInTheDocument()
  expect(screen.getByText("Still valid")).toBeInTheDocument()
  await user.click(screen.getAllByRole("button", { name: "Revoke" })[0])
  expect(revokePortalOAuthSession).not.toHaveBeenCalled()
  vi.mocked(portalOAuthSessions).mockResolvedValue({ total_logins: 2, active_sessions: 0, total: 0, sessions: [] })
  await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Revoke" }))
  await waitFor(() => expect(revokePortalOAuthSession).toHaveBeenCalledWith(7, expect.anything()))
  expect(await screen.findByText("No authorized sessions")).toBeInTheDocument()
  expect(screen.getAllByText("0")[0]).toBeInTheDocument()
})

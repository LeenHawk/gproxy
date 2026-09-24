import { act, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import "@/i18n"
import { CredentialLoginDialog } from "./login"
import * as api from "@/api/credentials"
import type { CredentialProviderDto } from "@/generated/app"
vi.mock("./ownership", () => ({ useOwnerChoices: () => ({ defaultOwner: "org:o1", choices: [{ value: "org:o1", label: "Acme" }] }), ownerColumns: () => ({ organizationId: "o1", teamId: null, userId: null }) }))
vi.mock("@/api/credentials", () => ({ startDevice: vi.fn(), pollDevice: vi.fn(), startAuthCode: vi.fn(), completeAuthCode: vi.fn(), exchangeCookie: vi.fn() }))
const provider: CredentialProviderDto = { id: "p", name: "Provider", channel: "test", enabled: true, loginModes: ["device_code"], capabilities: { refresh: false, quotaQuery: false, quotaReset: false, services: false, websocket: false } }
afterEach(() => { vi.useRealTimers(); vi.resetAllMocks() })
async function mount() {
  vi.useFakeTimers()
  vi.mocked(api.startDevice).mockResolvedValue({ loginSessionId: "session", userCode: "CODE", verificationUri: "https://example.test", verificationUriComplete: null, intervalSecs: 2, expiresAtMs: null })
  const close = vi.fn()
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  const view = render(<QueryClientProvider client={client}><CredentialLoginDialog provider={provider} onClose={close} /></QueryClientProvider>)
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Start sign-in" })); await vi.advanceTimersByTimeAsync(0) })
  return { close, client, ...view }
}
describe("device sign-in lifecycle", () => {
  it("honors a raised poll interval and closes after successful cache invalidation", async () => {
    vi.mocked(api.pollDevice).mockResolvedValueOnce({ status: "pending", intervalSecs: 7 }).mockResolvedValueOnce({ status: "ready", credentialId: "c" })
    const { close } = await mount()
    await act(async () => { await vi.advanceTimersByTimeAsync(1999) }); expect(api.pollDevice).not.toHaveBeenCalled()
    await act(async () => { await vi.advanceTimersByTimeAsync(1) }); expect(api.pollDevice).toHaveBeenCalledTimes(1)
    await act(async () => { await vi.advanceTimersByTimeAsync(6999) }); expect(api.pollDevice).toHaveBeenCalledTimes(1)
    await act(async () => { await vi.advanceTimersByTimeAsync(1) }); expect(api.pollDevice).toHaveBeenCalledTimes(2)
    expect(close).toHaveBeenCalledOnce()
  })
  it("stops scheduling after the login window is closed", async () => {
    const { unmount } = await mount()
    unmount()
    await act(async () => { await vi.advanceTimersByTimeAsync(30_000) })
    expect(api.pollDevice).not.toHaveBeenCalled()
  })
})

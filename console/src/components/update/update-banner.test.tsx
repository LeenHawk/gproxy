import type { InstanceSettingsDto } from "@/generated/InstanceSettingsDto"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import "@/i18n"
import { instanceSettings } from "@/api/control"
import { updateStatus } from "@/api/native"
import { UpdateBanner } from "./update-banner"

vi.mock("@/api/control", () => ({ instanceSettings: vi.fn() }))
vi.mock("@/api/native", () => ({ updateStatus: vi.fn() }))

const settings = vi.mocked(instanceSettings)
const status = vi.mocked(updateStatus)

function renderBanner() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return render(<QueryClientProvider client={client}><UpdateBanner /></QueryClientProvider>)
}

describe("automatic update banner", () => {
  beforeEach(() => {
    window.localStorage.clear()
    settings.mockReset()
    status.mockReset()
  })

  it("does not check when automatic checks are disabled", async () => {
    settings.mockResolvedValue({ enable_auto_update_check: false } as InstanceSettingsDto)
    renderBanner()
    await waitFor(() => expect(settings).toHaveBeenCalledOnce())
    expect(status).not.toHaveBeenCalled()
  })

  it("shows an available update returned by the selected channel", async () => {
    settings.mockResolvedValue({ enable_auto_update_check: true } as InstanceSettingsDto)
    status.mockResolvedValue({
      current: "3.0.0-alpha.0",
      latest: "3.0.0-alpha.1",
      available: true,
      channel: "dev",
      target: "x86_64-unknown-linux-gnu",
      notes: null,
      rollback_available: false,
      restart: "supervisor",
    })
    renderBanner()
    expect(await screen.findByText("GPROXY 3.0.0-alpha.1 is available")).toBeVisible()
  })
})

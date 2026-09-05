import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { beforeEach, describe, expect, it, vi } from "vitest"
import "@/i18n"
import { applyUpdate, rollbackUpdate, updateStatus } from "@/api/native"
import { UpdatePanel } from "./update-panel"

vi.mock("@/api/native", () => ({
  applyUpdate: vi.fn(),
  rollbackUpdate: vi.fn(),
  updateStatus: vi.fn(),
}))

const apply = vi.mocked(applyUpdate)
const rollback = vi.mocked(rollbackUpdate)
const status = vi.mocked(updateStatus)

function renderPanel() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return render(<QueryClientProvider client={client}><UpdatePanel /></QueryClientProvider>)
}

describe("native update panel", () => {
  beforeEach(() => {
    apply.mockReset()
    rollback.mockReset()
    status.mockReset()
  })

  it("offers automatic restart and opens release notes in a dialog", async () => {
    status.mockResolvedValue({
      current: "3.0.0-alpha.1",
      latest: "3.0.0-alpha.2",
      available: true,
      channel: "dev",
      target: "x86_64-unknown-linux-gnu",
      notes: "## Changes\n\n- Restart after updating",
      rollback_available: false,
      restart: "re-exec",
    })
    const user = userEvent.setup()
    renderPanel()

    await user.click(screen.getByRole("button", { name: "Check for updates" }))
    expect(await screen.findByRole("button", { name: "Update and restart" })).toBeEnabled()

    await user.click(screen.getByRole("button", { name: "View release notes" }))
    const dialog = screen.getByRole("dialog", { name: "Release notes" })
    expect(dialog).toHaveTextContent("GPROXY 3.0.0-alpha.1 → 3.0.0-alpha.2")
    expect(dialog).toHaveTextContent("Restart after updating")
  })
})

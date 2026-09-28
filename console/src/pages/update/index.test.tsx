import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { beforeEach, expect, it, vi } from "vitest"
import "@/i18n"
import { readSettings, saveSettings, instanceInfo } from "@/api/settings"
import { checkUpdate, updateSchedule, type UpdateReport } from "@/api/update"
import type { SettingsDto } from "@/generated/sdk"
import { UpdatePage } from "./index"

vi.mock("@/api/settings", () => ({ SETTINGS_KEY: ["settings"], INFO_KEY: ["info"], readSettings: vi.fn(), saveSettings: vi.fn(), instanceInfo: vi.fn() }))
vi.mock("@/api/update", () => ({ checkUpdate: vi.fn(), applyUpdate: vi.fn(), rollbackUpdate: vi.fn(), updateSchedule: vi.fn() }))

const report: UpdateReport = { current: "4.0.0-dev", latest: "new-commit", available: true, channel: "dev", source: "github", target: "x86_64-unknown-linux-gnu", notes_url: null, notes: null, restart: "none", rollback_available: false, checked_at_ms: 1 }
let stored: SettingsDto

function mount() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  return render(<QueryClientProvider client={client}><UpdatePage /></QueryClientProvider>)
}

beforeEach(() => {
  vi.resetAllMocks()
  stored = { instance: { updateChannel: "dev", updateSource: "github" }, logging: {} } as SettingsDto
  vi.mocked(instanceInfo).mockResolvedValue({ instanceName: "test", version: "4.0.0-dev", hash: "commit" })
  vi.mocked(readSettings).mockImplementation(async () => stored)
  vi.mocked(updateSchedule).mockResolvedValue({ last_check: report, last_error: null, interval_secs: 21600, automatic: false, channel: "dev", source: "github" })
  vi.mocked(saveSettings).mockImplementation(async (patch) => {
    stored = { ...stored, instance: { ...stored.instance, ...patch.instance } } as SettingsDto
    return stored
  })
  vi.mocked(checkUpdate).mockImplementation(async (selection) => ({ ...report, ...selection }))
})

it("checks the selected source and does not offer an install from the previous source", async () => {
  mount()
  await screen.findByText("Update available: new-commit")
  fireEvent.keyDown(screen.getByRole("combobox", { name: "Update source" }), { key: "Enter" })
  fireEvent.click(await screen.findByRole("option", { name: "CNB" }))
  expect(screen.getByRole("button", { name: "Install update" })).toBeDisabled()
  expect(screen.queryByText("Update available: new-commit")).not.toBeInTheDocument()
  fireEvent.click(screen.getByRole("button", { name: "Check for updates" }))
  await screen.findByText("Update available: new-commit")
  expect(vi.mocked(checkUpdate).mock.calls[0][0]).toEqual({ channel: "dev", source: "cnb" })
  expect(screen.getByRole("button", { name: "Install update" })).toBeEnabled()
})

it("saves both source and channel and restores them when the page is reopened", async () => {
  const view = mount()
  await screen.findByText("Update available: new-commit")
  fireEvent.keyDown(screen.getByRole("combobox", { name: "Update source" }), { key: "Enter" })
  fireEvent.click(await screen.findByRole("option", { name: "CNB" }))
  fireEvent.click(screen.getByRole("button", { name: "Save" }))
  await waitFor(() => expect(saveSettings).toHaveBeenCalledWith({ instance: { updateChannel: "dev", updateSource: "cnb" } }))
  await waitFor(() => expect(screen.getByRole("button", { name: "Save" })).toBeDisabled())
  view.unmount()
  mount()
  await waitFor(() => expect(screen.getByRole("combobox", { name: "Update source" })).toHaveTextContent("CNB"))
  expect(screen.getByRole("combobox", { name: "Update channel" })).toHaveTextContent("Dev")
})

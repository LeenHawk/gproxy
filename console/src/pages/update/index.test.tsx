import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { beforeEach, expect, it, vi } from "vitest"
import "@/i18n"
import { readSettings, saveSettings, instanceInfo } from "@/api/settings"
import { announcements, checkUpdate, updateSchedule, updateProgress, type Announcement, type UpdateReport } from "@/api/update"
import type { SettingsDto } from "@/generated/sdk"
import { UpdatePage } from "./index"

vi.mock("@/api/settings", () => ({ SETTINGS_KEY: ["settings"], INFO_KEY: ["info"], readSettings: vi.fn(), saveSettings: vi.fn(), instanceInfo: vi.fn() }))
vi.mock("@/api/update", () => ({ ANNOUNCEMENTS_KEY: ["update", "announcements"], announcements: vi.fn(), checkUpdate: vi.fn(), applyUpdate: vi.fn(), rollbackUpdate: vi.fn(), updateSchedule: vi.fn(), updateProgress: vi.fn() }))

const report: UpdateReport = { current: "4.0.0-dev", latest: "new-commit", available: true, channel: "dev", source: "github", target: "x86_64-unknown-linux-gnu", notes_url: null, notes: null, restart: "none", rollback_available: false, checked_at_ms: 1 }
let stored: SettingsDto

function mount() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  return render(<QueryClientProvider client={client}><UpdatePage /></QueryClientProvider>)
}

beforeEach(() => {
  vi.resetAllMocks()
  vi.mocked(updateProgress).mockResolvedValue(null)
  vi.mocked(announcements).mockResolvedValue([])
  stored = { instance: { updateChannel: "dev", updateSource: "github" }, logging: {} } as SettingsDto
  vi.mocked(instanceInfo).mockResolvedValue({ instanceName: "test", version: "4.0.0-dev", hash: "commit" })
  vi.mocked(readSettings).mockImplementation(async () => stored)
  vi.mocked(updateSchedule).mockImplementation(async () => ({ last_check: report, last_error: null, interval_secs: 21600, automatic: false, verify_signature: stored.instance.updateVerifySignature ?? true, channel: "dev", source: "github" }))
  vi.mocked(saveSettings).mockImplementation(async (patch) => {
    stored = { ...stored, instance: { ...stored.instance, ...patch.instance } } as SettingsDto
    return stored
  })
  vi.mocked(checkUpdate).mockImplementation(async (selection) => ({ ...report, ...selection }))
})

it.each([["cnb", "CNB"], ["gitlab", "GitLab"]])("checks %s and does not offer an install from the previous source", async (source, label) => {
  mount()
  await screen.findByText("Update available: new-commit")
  fireEvent.keyDown(screen.getByRole("combobox", { name: "Update source" }), { key: "Enter" })
  fireEvent.click(await screen.findByRole("option", { name: label }))
  expect(screen.getByRole("button", { name: "Install update" })).toBeDisabled()
  expect(screen.queryByText("Update available: new-commit")).not.toBeInTheDocument()
  fireEvent.click(screen.getByRole("button", { name: "Check for updates" }))
  await screen.findByText("Update available: new-commit")
  expect(vi.mocked(checkUpdate).mock.calls[0][0]).toEqual({ channel: "dev", source })
  expect(screen.getByRole("button", { name: "Install update" })).toBeEnabled()
})

it.each([["cnb", "CNB"], ["gitlab", "GitLab"]])("saves %s and channel and restores them when the page is reopened", async (source, label) => {
  const view = mount()
  await screen.findByText("Update available: new-commit")
  fireEvent.keyDown(screen.getByRole("combobox", { name: "Update source" }), { key: "Enter" })
  fireEvent.click(await screen.findByRole("option", { name: label }))
  fireEvent.click(screen.getByRole("button", { name: "Save" }))
  await waitFor(() => expect(saveSettings).toHaveBeenCalledWith({ instance: { updateChannel: "dev", updateSource: source } }))
  await waitFor(() => expect(screen.getByRole("button", { name: "Save" })).toBeDisabled())
  view.unmount()
  mount()
  await waitFor(() => expect(screen.getByRole("combobox", { name: "Update source" })).toHaveTextContent(label))
  expect(screen.getByRole("combobox", { name: "Update channel" })).toHaveTextContent("Dev")
})

it("saves the signature switch immediately and restores it when reopened", async () => {
  const view = mount()
  const toggle = await screen.findByRole("switch", { name: "Verify update signatures" })
  await waitFor(() => expect(toggle).toBeEnabled())
  expect(toggle).toBeChecked()
  fireEvent.click(toggle)
  await waitFor(() => expect(saveSettings).toHaveBeenCalledWith({ instance: { updateVerifySignature: false } }))
  await waitFor(() => expect(toggle).not.toBeChecked())
  view.unmount()
  mount()
  const restored = await screen.findByRole("switch", { name: "Verify update signatures" })
  await waitFor(() => expect(restored).toBeEnabled())
  expect(restored).not.toBeChecked()
  fireEvent.click(restored)
  await waitFor(() => expect(saveSettings).toHaveBeenLastCalledWith({ instance: { updateVerifySignature: true } }))
  await waitFor(() => expect(restored).toBeChecked())
})

it("shows the build, the project links and an empty announcement list", async () => {
  mount()
  await screen.findByText("Update available: new-commit")
  expect(screen.getByText("4.0.0-dev")).toBeInTheDocument()
  expect(screen.getByRole("link", { name: "Source code" })).toHaveAttribute("href", "https://github.com/LeenHawk/gproxy")
  expect(screen.getByRole("link", { name: /Sponsor LeenHawk/ })).toBeInTheDocument()
  await screen.findByText("No announcements for this build.")
})

it("renders announcements in the console language, falling back to English, with the body's Markdown subset", async () => {
  const notices: Array<Announcement> = [
    { id: "advisory", severity: "critical", published_at: "2026-10-01T00:00:00Z", affects: "<4.0.4", content: {
      en: { title: "Upgrade to 4.0.4", body: "## Why\n\nA **credential** leak in `/v1/models`.\n\n- Upgrade now\n- Rotate keys\n\n> Thanks to the reporter." },
      "zh-CN": { title: "请升级到 4.0.4", body: "**凭证**泄露。" },
    } },
    { id: "note", severity: "info", published_at: "2026-09-20T00:00:00Z", content: { en: { title: "English only", body: "Plain text." } } },
  ]
  vi.mocked(announcements).mockResolvedValue(notices)
  mount()
  await screen.findByText("Upgrade to 4.0.4")
  expect(screen.getByText("Critical")).toBeInTheDocument()
  expect(screen.getByRole("heading", { level: 3, name: "Why" })).toBeInTheDocument()
  expect(screen.getByText("credential").tagName).toBe("STRONG")
  expect(screen.getByText("/v1/models").tagName).toBe("CODE")
  expect(screen.getAllByRole("listitem").map((item) => item.textContent)).toEqual(expect.arrayContaining(["Upgrade now", "Rotate keys"]))
  expect(screen.getByText("Thanks to the reporter.").closest("blockquote")).not.toBeNull()
  expect(screen.getByText("English only")).toBeInTheDocument()

  const { default: i18n } = await import("@/i18n")
  await i18n.changeLanguage("zh-CN")
  await screen.findByText("请升级到 4.0.4")
  expect(screen.getByText("凭证").tagName).toBe("STRONG")
  // No zh-CN entry: the English one stands in.
  expect(screen.getByText("English only")).toBeInTheDocument()
  await i18n.changeLanguage("en")
})

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { describe, expect, it, vi } from "vitest"
import "@/i18n"
import { ApplicationPresetButton } from "@/components/rules/application-preset-button"
import { RuleSetForm } from "@/components/rules/rule-set-form"

const control = vi.hoisted(() => ({
  applyRulePreset: vi.fn().mockResolvedValue({ id: 8 }),
  rulePresets: vi.fn().mockResolvedValue([{
    id: "opencode",
    name: "OpenCode",
    description: "gproxy:preset:opencode:v1",
    category: "application",
    rules: [],
  }]),
}))

vi.mock("@/api/control", () => control)

describe("RuleSetForm", () => {
  it("creates a rule set inline and returns its new id", async () => {
    const save = vi.fn().mockResolvedValue({ id: 8 })
    const saved = vi.fn()
    render(<RuleSetForm saving={false} onSave={save} onSaved={saved} />)

    const user = userEvent.setup()
    await user.type(screen.getByRole("textbox", { name: "Name" }), "New Rules")
    await user.type(screen.getByRole("textbox", { name: "Description" }), "Inline settings")
    await user.click(screen.getByRole("button", { name: "Create" }))

    await waitFor(() => expect(saved).toHaveBeenCalledWith({ id: 8 }))
    expect(save).toHaveBeenCalledWith({ name: "New Rules", description: "Inline settings", enabled: true }, undefined)
    expect(screen.queryByRole("dialog")).toBeNull()
  })

  it("applies compatibility rules to the selected rule set", async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    render(<QueryClientProvider client={client}><ApplicationPresetButton ruleSetId={8} /></QueryClientProvider>)

    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "Apply compatibility preset" }))
    await user.click(await screen.findByRole("menuitem", { name: "OpenCode" }))

    await waitFor(() => expect(control.applyRulePreset).toHaveBeenCalledWith(8, "opencode"))
  })
})

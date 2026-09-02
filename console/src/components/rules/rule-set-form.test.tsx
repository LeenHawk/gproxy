import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { describe, expect, it, vi } from "vitest"
import "@/i18n"
import { RuleSetForm } from "@/components/rules/rule-set-form"

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
})

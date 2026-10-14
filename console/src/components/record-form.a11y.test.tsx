import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { describe, expect, it, vi } from "vitest"
import "@/i18n"
import { RecordEditor } from "./record-form"

const fields = [
  { name: "name", label: "Account name", kind: "text" as const, required: true, description: "Use a recognizable name." },
  { name: "metadata", label: "Metadata", kind: "json" as const, description: "Enter a JSON object." },
]

describe("record form accessibility", () => {
  it("names and describes each field, including two editors on the same page", () => {
    render(<><RecordEditor fields={fields} mode="create" onSubmit={vi.fn()} /><RecordEditor fields={fields} mode="create" onSubmit={vi.fn()} /></>)
    const names = screen.getAllByRole("textbox", { name: "Account name" })
    expect(names[0]).toBeRequired()
    expect(names[0]).toHaveAccessibleDescription("Use a recognizable name.")
    expect(names[0].id).not.toBe(names[1].id)
    expect(screen.getAllByRole("textbox", { name: "Metadata" })[0]).toHaveAccessibleDescription("Enter a JSON object.")
  })

  it("identifies a missing field and moves focus there without submitting", async () => {
    const save = vi.fn()
    render(<RecordEditor fields={fields} mode="create" onSubmit={save} />)
    await userEvent.click(screen.getByRole("button", { name: "Create" }))
    const name = screen.getByRole("textbox", { name: "Account name" })
    await waitFor(() => expect(name).toHaveFocus())
    expect(name).toBeInvalid()
    expect(name).toHaveAccessibleDescription("Use a recognizable name. This field is required.")
    expect(save).not.toHaveBeenCalled()
  })

  it("opens the advanced fields to expose and focus a JSON error", async () => {
    const save = vi.fn()
    render(<RecordEditor fields={fields} original={{ name: "Account", metadata: {} }} mode="edit" advancedFields={["metadata"]} onSubmit={save} />)
    fireEvent.change(screen.getByLabelText("Metadata"), { target: { value: "{" } })
    await userEvent.click(screen.getByRole("button", { name: "Save" }))
    const input = screen.getByRole("textbox", { name: "Metadata" })
    await waitFor(() => expect(input).toHaveFocus())
    expect(input).toBeInvalid()
    expect(input).toHaveAccessibleDescription(/Invalid JSON/)
    expect(input.closest("details")).toHaveAttribute("open")
    expect(save).not.toHaveBeenCalled()
    fireEvent.change(input, { target: { value: '{"updated":true}' } })
    await userEvent.click(screen.getByRole("button", { name: "Save" }))
    expect(save).toHaveBeenCalledWith({ metadata: { updated: true } })
  })

  it("submits from a text field with Enter", async () => {
    const save = vi.fn()
    render(<RecordEditor fields={fields} mode="create" onSubmit={save} />)
    await userEvent.type(screen.getByRole("textbox", { name: "Account name" }), "Keyboard user{Enter}")
    expect(save).toHaveBeenCalledWith({ name: "Keyboard user" })
  })
})

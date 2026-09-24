import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import "@/i18n"
import { KeyCreateDialog } from "./create-dialog"
const fields = [{ name: "name", kind: "text" as const, required: true }]
describe("create a key with its budget", () => {
  it("submits key details and budget in one request and validates the amount before submit", () => {
    const submit = vi.fn()
    render(<KeyCreateDialog open title="New key" fields={fields} canSetBudget onOpenChange={() => {}} onSubmit={submit} pending={false} error={null} />)
    fireEvent.change(screen.getByLabelText(/Name/), { target: { value: "Application key" } })
    fireEvent.click(screen.getByRole("switch", { name: "Set a budget for this key" }))
    expect(screen.getByRole("button", { name: "Create" })).toBeDisabled()
    fireEvent.change(screen.getByLabelText("Cost limit (USD)"), { target: { value: "25.50" } })
    fireEvent.click(screen.getByRole("button", { name: "Create" }))
    expect(submit).toHaveBeenCalledOnce()
    expect(submit).toHaveBeenCalledWith({ name: "Application key", budget: { limitValue: "25.50", windowKey: "Default budget", period: "1m", periodSeconds: null, anchorAtMs: null, modelPattern: null } })
  })
  it("does not expose or submit an operator budget without permission", () => {
    const submit = vi.fn()
    render(<KeyCreateDialog open title="New key" fields={fields} canSetBudget={false} onOpenChange={() => {}} onSubmit={submit} pending={false} error={null} />)
    expect(screen.queryByRole("switch")).not.toBeInTheDocument()
    expect(screen.getByText(/administrator can set this key/)).toBeInTheDocument()
    fireEvent.change(screen.getByLabelText(/Name/), { target: { value: "Personal key" } })
    fireEvent.click(screen.getByRole("button", { name: "Create" }))
    expect(submit).toHaveBeenCalledWith({ name: "Personal key" })
  })
  it("retains budget inputs after a server rejection", () => {
    const submit = vi.fn()
    const props = { open: true, title: "New key", fields, canSetBudget: true, onOpenChange: () => {}, onSubmit: submit, pending: false }
    const { rerender } = render(<KeyCreateDialog {...props} error={null} />)
    fireEvent.change(screen.getByLabelText(/Name/), { target: { value: "Retryable key" } })
    fireEvent.click(screen.getByRole("switch", { name: "Set a budget for this key" }))
    fireEvent.change(screen.getByLabelText("Cost limit (USD)"), { target: { value: "8" } })
    rerender(<KeyCreateDialog {...props} error={new Error("Budget validation failed")} />)
    expect(screen.getByText("Budget validation failed")).toBeInTheDocument()
    expect(screen.getByLabelText("Cost limit (USD)")).toHaveValue("8")
    expect(screen.getAllByRole("dialog")).toHaveLength(1)
  })
})

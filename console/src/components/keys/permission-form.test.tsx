import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import "@/i18n"
import { PermissionForm } from "@/components/keys/permission-form"

describe("PermissionForm", () => {
  it("submits only to the fixed detail-page subject", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined)
    render(<PermissionForm providers={[]} groups={[]} pending={false} fixedSubject={{ kind: "team", id: 7 }} onSubmit={onSubmit} />)

    expect(screen.queryByText("Applies to")).toBeNull()
    fireEvent.click(screen.getByRole("button", { name: "Add permission" }))
    await waitFor(() => expect(onSubmit).toHaveBeenCalledWith({ subject_kind: "team", subject_id: 7, provider_id: null, operation_group: null, allowed: true }))
  })
})

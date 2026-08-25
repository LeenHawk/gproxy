import { act, fireEvent, render, screen } from "@testing-library/react"
import { toast } from "sonner"
import { afterEach, describe, expect, it, vi } from "vitest"
import "@/i18n"
import type { UserKeyDto } from "@/generated/UserKeyDto"
import { KeySecretCell } from "@/components/keys/key-secret-cell"

vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }))

const record: UserKeyDto = {
  id: 7,
  user_id: 11,
  prefix: "visible-prefix:",
  label: null,
  revealable: true,
  expires_at: null,
  enabled: true,
}
const secret = "revealed sentinel text"
let clipboardDescriptor: PropertyDescriptor | undefined

afterEach(() => {
  if (clipboardDescriptor) Object.defineProperty(navigator, "clipboard", clipboardDescriptor)
  else Reflect.deleteProperty(navigator, "clipboard")
  clipboardDescriptor = undefined
  vi.useRealTimers()
  vi.restoreAllMocks()
  vi.clearAllMocks()
})

describe("KeySecretCell", () => {
  it("copies the revealed value and automatically masks it again", async () => {
    vi.useFakeTimers()
    const writeText = vi.fn().mockResolvedValue(undefined)
    clipboardDescriptor = Object.getOwnPropertyDescriptor(navigator, "clipboard")
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } })
    const reveal = vi.fn().mockResolvedValue({ id: record.id, api_key: secret, revealed_at: 123 })
    render(<KeySecretCell record={record} reveal={reveal} remaskMs={1_000} />)

    expect(screen.queryByText(secret)).not.toBeInTheDocument()
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Reveal key" })) })
    expect(screen.getByText(secret)).toBeInTheDocument()
    expect(reveal).toHaveBeenCalledOnce()

    await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Copy full key" })) })
    expect(writeText).toHaveBeenCalledWith(secret)
    expect(toast.success).toHaveBeenCalledWith("Full key copied.")

    act(() => vi.advanceTimersByTime(1_000))
    expect(screen.queryByText(secret)).not.toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Reveal key" })).toBeInTheDocument()
  })
})

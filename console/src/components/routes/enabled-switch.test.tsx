import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { toast } from "sonner"
import { afterEach, expect, test, vi } from "vitest"
import { EnabledSwitch } from "@/components/routes/enabled-switch"

vi.mock("sonner", () => ({ toast: { error: vi.fn() } }))

afterEach(() => {
  cleanup()
  vi.clearAllMocks()
})

function renderSwitch(onChange: (enabled: boolean) => Promise<unknown>, onChanged = vi.fn()) {
  const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
  render(
    <QueryClientProvider client={client}>
      <EnabledSwitch
        checked={false}
        label="Enabled"
        errorMessage="Save failed"
        onChange={onChange}
        onChanged={onChanged}
      />
    </QueryClientProvider>,
  )
  return onChanged
}

test("shows the requested state while an enable update is pending", async () => {
  const user = userEvent.setup()
  let finish: () => void = () => {}
  const onChange = vi.fn(() => new Promise<void>((resolve) => { finish = resolve }))
  const onChanged = renderSwitch(onChange)
  const control = screen.getByRole("switch")

  await user.click(control)
  await waitFor(() => expect(onChange).toHaveBeenCalledWith(true))
  expect(control).toBeChecked()
  expect(control).toBeDisabled()

  finish()
  await waitFor(() => expect(onChanged).toHaveBeenCalledOnce())
})

test("restores the stored state and reports an enable update failure", async () => {
  const user = userEvent.setup()
  const onChanged = renderSwitch(() => Promise.reject(new Error("failed")))

  await user.click(screen.getByRole("switch"))
  await waitFor(() => expect(toast.error).toHaveBeenCalledWith("Save failed"))
  expect(screen.getByRole("switch")).not.toBeChecked()
  expect(onChanged).not.toHaveBeenCalled()
})

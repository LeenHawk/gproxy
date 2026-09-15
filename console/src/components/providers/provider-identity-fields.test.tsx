import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"
import { expect, test, vi } from "vitest"
import "@/i18n"
import { ProviderIdentityFields } from "@/components/providers/provider-identity-fields"

vi.mock("@/components/connectivity-test", () => ({ ConnectivityTest: () => null }))

function Harness({ initial = "round_robin" }: { initial?: string }) {
  const [strategy, setStrategy] = useState(initial)
  return <>
    <ProviderIdentityFields id="test" label="" strategy={strategy} proxyUrl="" onLabel={() => {}} onProxy={() => {}} onStrategy={setStrategy} />
    <output aria-label="Saved strategy">{strategy}</output>
  </>
}

test("offers earliest reset as an opt-in strategy and preserves saved selection", async () => {
  const user = userEvent.setup()
  const view = render(<Harness />)
  const strategy = screen.getByRole("combobox", { name: "Credential strategy" })
  expect(strategy).toHaveTextContent("Round robin")
  strategy.focus()
  await user.keyboard("{ArrowDown}{End}{Enter}")
  expect(screen.getByRole("status", { name: "Saved strategy" })).toHaveTextContent("earliest_reset")
  expect(strategy).toHaveTextContent("Earliest quota reset first")
  view.unmount()
  render(<Harness initial="earliest_reset" />)
  expect(screen.getByRole("combobox", { name: "Credential strategy" })).toHaveTextContent("Earliest quota reset first")
})

import type { TrafficPolicyDto } from "@/generated/TrafficPolicyDto"
import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"
import { expect, test } from "vitest"
import "@/i18n"
import { TrafficPolicyFields } from "@/components/providers/traffic-policy-fields"

const defaults: TrafficPolicyDto = {
  request_headers: ["openai-beta"],
  response_headers: ["x-request-id", "x-ratelimit-*"],
  request_query: ["limit"],
}

function Harness() {
  const [value, setValue] = useState<TrafficPolicyDto | null>(null)
  return <TrafficPolicyFields id="test" defaults={defaults} value={value} onChange={setValue} />
}

test("inherits channel metadata policy until the provider override is enabled", async () => {
  const user = userEvent.setup()
  render(<Harness />)
  const fields = screen.getAllByRole("textbox")
  expect(fields).toHaveLength(3)
  expect(fields[0]).toBeDisabled()
  expect(fields[0]).toHaveValue("openai-beta")

  await user.click(screen.getByRole("switch", { name: "Override channel defaults" }))
  expect(fields[0]).toBeEnabled()
  await user.clear(fields[0])
  await user.type(fields[0], "x-custom-*")
  expect(fields[0]).toHaveValue("x-custom-*")

  await user.click(screen.getByRole("button", { name: "Restore channel defaults" }))
  expect(fields[0]).toBeDisabled()
  expect(fields[0]).toHaveValue("openai-beta")
})

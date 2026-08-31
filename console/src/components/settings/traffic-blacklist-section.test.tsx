import type { TrafficBlacklistDto } from "@/generated/TrafficBlacklistDto"
import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"
import { expect, test } from "vitest"
import "@/i18n"
import { TrafficBlacklistSection } from "@/components/settings/traffic-blacklist-section"

const defaults: TrafficBlacklistDto = {
  request_headers: ["authorization", "cookie"],
  response_headers: ["set-cookie"],
  request_query: ["key"],
}

function Harness() {
  const [value, setValue] = useState<TrafficBlacklistDto>({
    request_headers: ["x-private-*"],
    response_headers: [],
    request_query: [],
  })
  return <TrafficBlacklistSection defaults={defaults} value={value} onChange={setValue} />
}

test("shows immutable defaults and restores additional blacklist entries", async () => {
  const user = userEvent.setup()
  render(<Harness />)
  const fields = screen.getAllByRole("textbox")
  expect(fields[0]).toBeDisabled()
  expect(fields[0]).toHaveValue("authorization\ncookie")
  expect(fields[1]).toHaveValue("x-private-*")

  await user.click(screen.getByRole("button", { name: "Restore built-in defaults" }))
  expect(fields[1]).toHaveValue("")
  expect(fields[0]).toHaveValue("authorization\ncookie")
})

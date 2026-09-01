import { act, fireEvent, render, screen } from "@testing-library/react"
import { toast } from "sonner"
import { afterEach, describe, expect, it, vi } from "vitest"
import "@/i18n"
import { Exchange } from "@/components/logs/exchange"

vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }))

let clipboardDescriptor: PropertyDescriptor | undefined

afterEach(() => {
  if (clipboardDescriptor) Object.defineProperty(navigator, "clipboard", clipboardDescriptor)
  else Reflect.deleteProperty(navigator, "clipboard")
  clipboardDescriptor = undefined
  vi.clearAllMocks()
})

describe("Exchange", () => {
  it("stacks bounded content sections and copies the displayed value", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined)
    clipboardDescriptor = Object.getOwnPropertyDescriptor(navigator, "clipboard")
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } })
    const { container } = render(
      <Exchange
        title="Client request"
        subtitle="request-1"
        method="POST"
        target="/v1/responses"
        requestHeaders={{ authorization: "[redacted]" }}
        requestBody={null}
        status={201}
        responseHeaders={{ "content-type": "application/json" }}
        responseBody={'{"ok":true}'}
        metrics={[
          { label: "Client IP", value: "198.51.100.7" },
          { label: "Duration", value: "2,000 ms" },
          { label: "Output TPS", value: "25 tok/s" },
        ]}
      />,
    )

    expect([...container.querySelectorAll("section h4")].map((node) => node.textContent)).toEqual([
      "Request headers",
      "Request body",
      "Response headers",
      "Response body",
    ])
    expect(container.querySelectorAll("pre.max-h-72")).toHaveLength(4)
    expect(screen.getAllByRole("button", { name: /^Copy / })).toHaveLength(4)
    expect(screen.getByRole("button", { name: "Copy Request body" })).toBeDisabled()
    expect(screen.getByLabelText("Response status: 201")).toBeInTheDocument()
    expect(screen.getByText("198.51.100.7")).toBeInTheDocument()
    expect(screen.getByText("25 tok/s")).toBeInTheDocument()

    await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Copy Request headers" })) })
    expect(writeText).toHaveBeenCalledWith('{\n  "authorization": "[redacted]"\n}')
    expect(toast.success).toHaveBeenCalledWith("Request headers copied.")
  })
})

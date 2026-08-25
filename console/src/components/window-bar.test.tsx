import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import "@/i18n"
import { WindowBar } from "@/components/window-bar"

describe("WindowBar", () => {
  it("renders inferred boundaries and partial coverage without false precision", () => {
    const { rerender } = render(
      <WindowBar
        label="5h"
        used="25"
        limit="100"
        boundary="unknown"
        confidence="partial"
        coverage="partial_lower_bound"
        unit="percent"
      />,
    )
    expect(screen.getByText("Boundary unknown")).toBeInTheDocument()
    expect(screen.getByText("Partial")).toBeInTheDocument()
    expect(screen.getByText("Partial coverage")).toBeInTheDocument()
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "25")

    rerender(<WindowBar label="5h" used="25" limit="100" coverage="unknown" unit="percent" />)
    expect(screen.getByText("Coverage unknown")).toBeInTheDocument()
    expect(screen.queryByText("Partial coverage")).not.toBeInTheDocument()
  })
})

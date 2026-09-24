import { useState } from "react"
import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import "@/i18n"
import { SearchableSelect } from "./searchable-select"

function Picker({ model = false }: { model?: boolean }) {
  const [value, setValue] = useState("old-id")
  return <><SearchableSelect id="picker" label="Target" value={value} options={[{ value: "alice-id", label: "Alice" }, { value: "bob-id", label: "Bob" }]} allowCustom={model} emptyLabel="All" emptyValue={model ? "*" : ""} onChange={setValue} /><output data-testid="value">{value}</output></>
}

describe("SearchableSelect", () => {
  it("searches names, saves IDs, preserves unknown values and clears scope", () => {
    render(<Picker />)
    fireEvent.click(screen.getByRole("combobox", { name: "Target" }))
    fireEvent.change(screen.getByPlaceholderText(/Search|搜索|搜尋/), { target: { value: "Alice" } })
    expect(screen.queryByRole("option", { name: "Bob" })).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole("option", { name: "Alice" }))
    expect(screen.getByTestId("value")).toHaveTextContent("alice-id")
    fireEvent.click(screen.getByRole("combobox", { name: "Target" }))
    fireEvent.click(screen.getByRole("option", { name: "All" }))
    expect(screen.getByTestId("value")).toBeEmptyDOMElement()
  })
  it("accepts a custom model pattern and restores the wildcard for all models", () => {
    render(<Picker model />)
    fireEvent.click(screen.getByRole("combobox", { name: "Target" }))
    fireEvent.change(screen.getByPlaceholderText(/Search|搜索|搜尋/), { target: { value: "gpt-*" } })
    fireEvent.click(screen.getByRole("option", { name: /gpt-\*/ }))
    expect(screen.getByTestId("value")).toHaveTextContent("gpt-*")
    fireEvent.click(screen.getByRole("combobox", { name: "Target" }))
    fireEvent.click(screen.getByRole("option", { name: "All" }))
    expect(screen.getByTestId("value")).toHaveTextContent("*")
  })
})

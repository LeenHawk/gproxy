import { useState } from "react"
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import "@/i18n"
import { useConfigBatch } from "./config-batch"
import type { ConfigFamily } from "@/api/config-family"

describe("configuration batch selection", () => {
  it("captures the submitted IDs and clears selection when the page context changes", async () => {
    let finish!: () => void
    const batch = vi.fn(() => new Promise<null[]>(resolve => { finish = () => resolve([]) }))
    const family: ConfigFamily<{ id: string }, object, { enabled: boolean }> = { path: "/credentials", batch, list: vi.fn(), get: vi.fn(), create: vi.fn(), update: vi.fn(), remove: vi.fn() }
    function Harness() {
      const [filter, setFilter] = useState("first")
      const bulk = useConfigBatch({ family, context: filter, rows: [{ id: filter }] })
      return <><input aria-label="Filter" value={filter} onChange={event => setFilter(event.target.value)} />{bulk.trigger}{bulk.toolbar}{bulk.checkbox(filter, filter)}</>
    }
    const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    render(<QueryClientProvider client={client}><Harness /></QueryClientProvider>)
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument()
    expect(screen.queryByRole("button", { name: "Disable" })).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole("button", { name: "Batch select" }))
    expect(screen.getByRole("button", { name: "Disable" })).toBeDisabled()
    fireEvent.click(screen.getByRole("checkbox", { name: "Select: first" }))
    fireEvent.click(screen.getByRole("button", { name: "Batch select" }))
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole("button", { name: "Batch select" }))
    expect(screen.getByRole("checkbox", { name: "Select: first" })).not.toBeChecked()
    fireEvent.click(screen.getByRole("checkbox", { name: "Batch select" }))
    fireEvent.click(screen.getByRole("button", { name: "Disable" }))
    await waitFor(() => expect(batch).toHaveBeenCalledWith([{ update: { id: "first", patch: { enabled: false } } }]))
    fireEvent.change(screen.getByLabelText("Filter"), { target: { value: "second" } })
    expect(screen.getByRole("checkbox", { name: "Select: second" })).not.toBeChecked()
    await act(async () => finish())
    expect(screen.getByRole("button", { name: "Disable" })).toBeDisabled()
  })
})

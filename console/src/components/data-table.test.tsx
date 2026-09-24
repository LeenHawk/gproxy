import { describe, expect, it, vi } from "vitest"
import { fireEvent, render, screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import "@/i18n"
import { DataTable } from "@/components/data-table"
import { CollectionPage } from "@/pages/identity/collection"
import type { ListFilter } from "@/api/admin"

const rows = Array.from({ length: 51 }, (_, i) => ({ id: `row-${i + 1}`, name: `Model ${i + 1}` }))
const columns = [{ key: "name", cell: (row: typeof rows[number]) => row.name }]
const count = () => within(screen.getByRole("table")).getAllByRole("row").length - 1
async function size(value: string) {
  fireEvent.keyDown(screen.getByRole("combobox", { name: "Per page" }), { key: "Enter" })
  fireEvent.click(await screen.findByRole("option", { name: value }))
}

describe("list pagination", () => {
  it("does not paginate settings or reference tables unless explicitly requested", () => {
    render(<DataTable rows={rows} columns={columns} rowKey={row => row.id} empty="Empty" />)
    expect(count()).toBe(51)
    expect(screen.queryByRole("combobox", { name: "Per page" })).not.toBeInTheDocument()
  })

  it("offers only 10/20/50 and resets to the first page when the size changes", async () => {
    const user = userEvent.setup()
    render(<DataTable paginate rows={rows} columns={columns} rowKey={row => row.id} empty="Empty" />)
    expect(count()).toBe(10)
    await size("20")
    expect(count()).toBe(20)
    await user.click(screen.getByRole("button", { name: "Next" }))
    expect(screen.getByText("Model 21")).toBeInTheDocument()
    await size("50")
    expect(count()).toBe(50)
    expect(screen.getByText("Model 1", { exact: true })).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "Next" }))
    expect(count()).toBe(1)
    expect(screen.getByRole("button", { name: "Next" })).toBeDisabled()
    fireEvent.keyDown(screen.getByRole("combobox"), { key: "Enter" })
    expect(screen.getAllByRole("option").map(option => option.textContent)).toEqual(["10", "20", "50"])
  })

  it("keeps a size selector on one page and resets filtered results without changing its size", async () => {
    const user = userEvent.setup()
    const { rerender } = render(<DataTable paginate rows={rows} columns={columns} rowKey={row => row.id} empty="Empty" resetPageKey="" />)
    await size("20")
    await user.click(screen.getByRole("button", { name: "Next" }))
    rerender(<DataTable paginate rows={rows.slice(0, 21)} columns={columns} rowKey={row => row.id} empty="Empty" resetPageKey="filtered" />)
    expect(count()).toBe(20)
    expect(screen.getByText("Model 1", { exact: true })).toBeInTheDocument()
    rerender(<DataTable paginate rows={rows} columns={columns} rowKey={row => row.id} empty="Empty" resetPageKey="" />)
    expect(screen.getByText("Model 1", { exact: true })).toBeInTheDocument()
    rerender(<DataTable paginate rows={rows.slice(0, 1)} columns={columns} rowKey={row => row.id} empty="Empty" resetPageKey="one" />)
    expect(screen.getByRole("combobox", { name: "Per page" })).toHaveTextContent("20")
    expect(screen.getByRole("button", { name: "Next" })).toBeDisabled()
  })

  it("sends page size and page to server-backed collections instead of paginating the response twice", async () => {
    const user = userEvent.setup()
    const list = vi.fn(async (filter: ListFilter) => {
      const page = filter.page ?? 1, pageSize = filter.pageSize ?? 10
      return { items: rows.slice((page - 1) * pageSize, page * pageSize), offset: (page - 1) * pageSize, limit: pageSize, total: rows.length }
    })
    const family = { path: "/rules", list, get: vi.fn(), create: vi.fn(), update: vi.fn(), remove: vi.fn() }
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    render(<QueryClientProvider client={client}><CollectionPage id="rules" family={family} columns={columns} fields={[]} rowId={row => row.id} rowLabel={row => row.name} /></QueryClientProvider>)
    await screen.findByText("Model 1", { exact: true })
    expect(count()).toBe(10)
    await size("20")
    await screen.findByText("Model 20", { exact: true })
    expect(count()).toBe(20)
    await user.click(screen.getByRole("button", { name: "Next" }))
    await screen.findByText("Model 21", { exact: true })
    expect(list).toHaveBeenLastCalledWith(expect.objectContaining({ page: 2, pageSize: 20 }))
    expect(screen.getAllByRole("combobox", { name: "Per page" })).toHaveLength(1)
  })
})

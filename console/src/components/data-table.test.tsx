import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { describe, expect, it, vi } from "vitest"
import { DataTable } from "@/components/data-table"

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, values?: { count?: number }) => key === "common.dataTable.selected" ? `${values?.count ?? 0} selected` : key,
  }),
}))

describe("DataTable", () => {
  it("requires a renderCard phone representation", () => {
    const invalidTable = () => (
      // @ts-expect-error renderCard is required so a table cannot ship without its phone form.
      <DataTable
        columns={[]}
        rows={[] as Array<{ id: number }>}
        rowKey={(row) => row.id}
        searchText={(row) => String(row.id)}
        empty={null}
        storageKey="type-contract"
      />
    )
    expect(invalidTable).toBeTypeOf("function")
  })

  it("keeps selection controls inside an explicit batch mode", async () => {
    const user = userEvent.setup()
    const onRowClick = vi.fn()
    render(
      <DataTable
        columns={[{ key: "name", label: "Name", header: "Name", cell: (row) => row.name }]}
        rows={[{ id: 1, name: "Alpha" }]}
        rowKey={(row) => row.id}
        searchText={(row) => row.name}
        renderCard={(row) => row.name}
        empty={null}
        storageKey="batch-mode"
        selectable
        createAction={<button>Add</button>}
        batchActions={(rows, onApplied) => <button onClick={onApplied}>Apply {rows.length}</button>}
        onRowClick={onRowClick}
      />,
    )

    expect(screen.queryByRole("checkbox", { name: "common.dataTable.selectAll" })).not.toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Add" })).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "common.batch.select" }))
    expect(screen.getByRole("checkbox", { name: "common.dataTable.selectAll" })).toBeInTheDocument()
    expect(screen.queryByRole("button", { name: "Add" })).not.toBeInTheDocument()
    expect(screen.getByText("0 selected")).toBeInTheDocument()

    await user.click(screen.getByRole("row", { name: /Alpha/ }))
    expect(screen.getByText("1 selected")).toBeInTheDocument()
    expect(onRowClick).not.toHaveBeenCalled()

    await user.click(screen.getByRole("button", { name: "Apply 1" }))
    expect(screen.queryByRole("checkbox", { name: "common.dataTable.selectAll" })).not.toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Add" })).toBeInTheDocument()
    await user.click(screen.getByRole("row", { name: /Alpha/ }))
    expect(onRowClick).toHaveBeenCalledTimes(1)
  })

  it("changes the number of visible items and resets to the first page", async () => {
    const user = userEvent.setup()
    const rows = Array.from({ length: 21 }, (_, index) => ({ id: index + 1, name: `Item ${index + 1}` }))
    render(
      <DataTable
        columns={[{ key: "name", label: "Name", header: "Name", cell: (row) => row.name }]}
        rows={rows}
        rowKey={(row) => row.id}
        searchText={(row) => row.name}
        renderCard={(row) => row.name}
        empty={null}
        storageKey="page-size"
      />,
    )

    expect(screen.queryByText("Item 11")).not.toBeInTheDocument()
    screen.getByRole("combobox", { name: "common.dataTable.itemsPerPage" }).focus()
    await user.keyboard("{Enter}{ArrowDown}{Enter}")
    expect(screen.getAllByText("Item 20")).toHaveLength(2)
    expect(screen.queryByText("Item 21")).not.toBeInTheDocument()

    await user.click(screen.getByRole("button", { name: "common.dataTable.next" }))
    expect(screen.getAllByText("Item 21")).toHaveLength(2)

    screen.getByRole("combobox", { name: "common.dataTable.itemsPerPage" }).focus()
    await user.keyboard("{Enter}{Home}{Enter}")
    expect(screen.getAllByText("Item 1")).toHaveLength(2)
    expect(screen.queryByText("Item 11")).not.toBeInTheDocument()
  })
})

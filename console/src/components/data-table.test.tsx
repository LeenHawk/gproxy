import { describe, expect, it } from "vitest"
import { DataTable } from "@/components/data-table"

describe("DataTable mobile contract", () => {
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
})

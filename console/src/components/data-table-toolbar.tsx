import type { ReactNode } from "react"
import { Columns3Icon, SearchIcon, XIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import type { DataTableColumn } from "@/components/data-table"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Input } from "@/components/ui/input"

export function DataTableToolbar<T>({
  query,
  onQuery,
  columns,
  hidden,
  onToggleColumn,
  selectedCount,
  onClearSelection,
  batchActions,
}: {
  query: string
  onQuery: (value: string) => void
  columns: Array<DataTableColumn<T>>
  hidden: Set<string>
  onToggleColumn: (key: string) => void
  selectedCount: number
  onClearSelection: () => void
  batchActions?: ReactNode
}) {
  const { t } = useTranslation()
  return (
    <div className="flex flex-wrap items-center gap-2">
      <div className="relative min-w-48 flex-1 sm:max-w-sm">
        <SearchIcon className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-muted-foreground" aria-hidden />
        <Input
          value={query}
          onChange={(event) => onQuery(event.target.value)}
          className="pl-8"
          placeholder={t("common.dataTable.search")}
          aria-label={t("common.dataTable.search")}
        />
      </div>
      {selectedCount > 0 ? (
        <div className="flex items-center gap-2 rounded-md border bg-muted px-2 py-1 text-xs">
          <span>{t("common.dataTable.selected", { count: selectedCount })}</span>
          {batchActions}
          <Button size="icon-xs" variant="ghost" onClick={onClearSelection} aria-label={t("common.dataTable.clearSelection")}>
            <XIcon aria-hidden />
          </Button>
        </div>
      ) : null}
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button size="icon-sm" variant="outline" aria-label={t("common.dataTable.columns")}>
            <Columns3Icon aria-hidden />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuGroup>
            {columns.map((column) => (
              <DropdownMenuCheckboxItem
                key={column.key}
                checked={!hidden.has(column.key)}
                onCheckedChange={() => onToggleColumn(column.key)}
                onSelect={(event) => event.preventDefault()}
              >
                {column.label}
              </DropdownMenuCheckboxItem>
            ))}
          </DropdownMenuGroup>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  )
}

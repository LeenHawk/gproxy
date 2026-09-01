import { useId, type ReactNode } from "react"
import { Columns3Icon } from "lucide-react"
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
  batchMode,
  onToggleBatch,
  createAction,
}: {
  query: string
  onQuery: (value: string) => void
  columns: Array<DataTableColumn<T>>
  hidden: Set<string>
  onToggleColumn: (key: string) => void
  batchMode?: boolean
  onToggleBatch?: () => void
  createAction?: ReactNode
}) {
  const { t } = useTranslation()
  const searchId = useId()
  return (
    <div className="flex flex-wrap items-center gap-2">
      <div className="min-w-48 flex-1 sm:max-w-sm">
        <Input
          id={searchId}
          value={query}
          onChange={(event) => onQuery(event.target.value)}
          placeholder={t("common.dataTable.search")}
          aria-label={t("common.dataTable.search")}
        />
      </div>
      {onToggleBatch ? <Button size="sm" variant="outline" onClick={onToggleBatch}>{t(`common.batch.${batchMode ? "cancel" : "select"}`)}</Button> : null}
      {!batchMode ? createAction : null}
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

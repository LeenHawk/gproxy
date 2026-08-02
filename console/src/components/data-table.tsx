import { type KeyboardEvent } from "react";
import type { ReactNode } from "react";
import { ColumnVisibilityToggle } from "@/components/column-visibility-toggle";
import { Card, CardContent } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
} from "@/components/ui/table";
import { cn } from "@/lib/utils";
import { useColumnVisibility } from "@/hooks/use-column-visibility";

export interface DataColumn<T> {
  key: string;
  label?: string;
  header: ReactNode;
  cell: (row: T) => ReactNode;
  className?: string;
}

export interface DataTableSelection {
  selectedIds: Set<string | number>;
  onToggle: (id: string | number) => void;
  onToggleAll: () => void;
  allSelected: boolean;
  indeterminate: boolean;
}

interface DataTableProps<T> {
  columns: DataColumn<T>[];
  rows: T[];
  rowKey: (row: T) => string | number;
  /** Mobile (<md) rendering — required so every list ships its phone form (spec §4). */
  renderCard: (row: T) => ReactNode;
  empty: ReactNode;
  /** Action buttons rendered inside cells/cards must call e.stopPropagation(),
   *  or the row click fires alongside the action. */
  onRowClick?: (row: T) => void;
  /** 选择模式;传入即显示复选框,且行点击改为切换选中(不触发 onRowClick)。 */
  selection?: DataTableSelection;
  columnToggle?: {
    storageKey: string;
    label: string;
    defaultHidden?: string[];
  };
}

export function DataTable<T>({
  columns,
  rows,
  rowKey,
  renderCard,
  empty,
  onRowClick,
  selection,
  columnToggle,
}: DataTableProps<T>) {
  const { hidden, setVisible } = useColumnVisibility(
    columnToggle?.storageKey,
    columnToggle?.defaultHidden,
  );
  const visibleColumns = columnToggle
    ? columns.filter((column) => !hidden.has(column.key))
    : columns;

  if (rows.length === 0) {
    return <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">{empty}</div>;
  }
  const selecting = selection !== undefined;
  // 选择模式下,行点击=切换选中;否则=导航(若有)。
  const rowClick = (row: T) => {
    if (selection) selection.onToggle(rowKey(row));
    else onRowClick?.(row);
  };
  const clickable = selecting || onRowClick !== undefined;
  // Keyboard activation for click-to-navigate rows (spec §12.2).
  const keyActivate = (row: T) => (e: KeyboardEvent) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      rowClick(row);
    }
  };
  return (
    <>
      <div className="hidden md:block">
        {columnToggle && (
          <div className="mb-2 flex justify-end">
            <ColumnVisibilityToggle
              columns={columns}
              hidden={hidden}
              label={columnToggle.label}
              onVisibleChange={setVisible}
            />
          </div>
        )}
        <div className="rounded-md border">
          <Table>
            <TableHeader>
              <TableRow>
                {selecting && (
                  <TableHead className="w-10">
                    <Checkbox
                      checked={selection.allSelected}
                      indeterminate={selection.indeterminate}
                      onCheckedChange={() => selection.onToggleAll()}
                      aria-label="select all"
                    />
                  </TableHead>
                )}
                {visibleColumns.map((col) => (
                  <TableHead key={col.key} className={col.className}>{col.header}</TableHead>
                ))}
              </TableRow>
            </TableHeader>
            <TableBody>
              {/* Clickable rows keep their implicit role="row" (NOT role="button") so
                  screen readers retain table semantics; tabIndex+onKeyDown activate them. */}
              {rows.map((row) => {
                const id = rowKey(row);
                return (
                  <TableRow
                    key={id}
                    data-state={selection?.selectedIds.has(id) ? "selected" : undefined}
                    className={cn(
                      clickable &&
                        "cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset",
                      selection?.selectedIds.has(id) && "bg-muted/50",
                    )}
                    {...(clickable
                      ? { tabIndex: 0, onClick: () => rowClick(row), onKeyDown: keyActivate(row) }
                      : {})}
                  >
                    {selecting && (
                      <TableCell className="w-10">
                        <Checkbox
                          checked={selection.selectedIds.has(id)}
                          onCheckedChange={() => selection.onToggle(id)}
                          aria-label="select row"
                        />
                      </TableCell>
                    )}
                    {visibleColumns.map((col) => (
                      <TableCell key={col.key} className={col.className}>{col.cell(row)}</TableCell>
                    ))}
                  </TableRow>
                );
              })}
            </TableBody>
          </Table>
        </div>
      </div>
      <div className="grid gap-2 md:hidden">
        {rows.map((row) => {
          const id = rowKey(row);
          return (
            <Card
              key={id}
              className={cn(
                clickable && "cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                selection?.selectedIds.has(id) && "ring-2 ring-primary",
              )}
              {...(clickable
                ? { role: "button", tabIndex: 0, onClick: () => rowClick(row), onKeyDown: keyActivate(row) }
                : {})}
            >
              <CardContent className="p-4">
                {selecting ? (
                  <div className="flex items-start gap-3">
                    <Checkbox
                      checked={selection.selectedIds.has(id)}
                      onCheckedChange={() => selection.onToggle(id)}
                      aria-label="select row"
                      className="mt-1"
                    />
                    <div className="min-w-0 flex-1">{renderCard(row)}</div>
                  </div>
                ) : (
                  renderCard(row)
                )}
              </CardContent>
            </Card>
          );
        })}
      </div>
    </>
  );
}

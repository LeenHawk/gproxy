import { Settings2 } from "lucide-react";
import type { DataColumn } from "@/components/data-table";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

interface ColumnVisibilityToggleProps<T> {
  columns: DataColumn<T>[];
  hidden: Set<string>;
  label: string;
  onVisibleChange: (key: string, visible: boolean) => void;
}

export function ColumnVisibilityToggle<T>({
  columns,
  hidden,
  label,
  onVisibleChange,
}: ColumnVisibilityToggleProps<T>) {
  const visibleCount = columns.filter((column) => !hidden.has(column.key)).length;

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="outline" size="sm">
          <Settings2 className="size-4" />
          {label}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-52">
        {columns.map((column) => {
          const visible = !hidden.has(column.key);
          return (
            <DropdownMenuCheckboxItem
              key={column.key}
              checked={visible}
              disabled={visible && visibleCount === 1}
              onCheckedChange={(checked) =>
                onVisibleChange(column.key, checked === true)
              }
            >
              {column.label ?? column.key}
            </DropdownMenuCheckboxItem>
          );
        })}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

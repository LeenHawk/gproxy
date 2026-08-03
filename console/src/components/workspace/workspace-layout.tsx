import { useMemo, useState, type CSSProperties, type ReactNode } from "react";
import { Search } from "lucide-react";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { VerticalResizeHandle } from "@/components/vertical-resize-handle";
import { useWorkspacePaneWidth } from "@/components/workspace/use-workspace-pane-width";

type WorkspaceBatch<Id extends string | number> = {
  active: boolean;
  selectedIds: ReadonlySet<Id>;
  entryLabel: string;
  cancelLabel: string;
  selectAllLabel: string;
  onEnter: () => void;
  onExit: () => void;
  onToggle: (id: Id) => void;
  onToggleAll: (ids: Id[]) => void;
  footer?: ReactNode;
};

export interface WorkspaceLayoutProps<T, Id extends string | number> {
  storageKey: string;
  title: string;
  items: T[];
  selectedId: Id | "new" | null;
  getId: (item: T) => Id;
  getSearchText: (item: T) => string;
  renderTitle: (item: T) => ReactNode;
  renderSummary: (item: T) => ReactNode;
  renderLink: (item: T, children: ReactNode, className: string) => ReactNode;
  renderAction?: (item: T) => ReactNode;
  searchPlaceholder: string;
  emptyLabel: string;
  emptyState: ReactNode;
  createAction?: ReactNode;
  filters?: ReactNode;
  mobileBack?: ReactNode;
  batch?: WorkspaceBatch<Id>;
  children: ReactNode;
}

/** Router-agnostic master/detail frame; callers own links, mutations, and route state. */
export function WorkspaceLayout<T, Id extends string | number>({
  storageKey,
  title,
  items,
  selectedId,
  getId,
  getSearchText,
  renderTitle,
  renderSummary,
  renderLink,
  renderAction,
  searchPlaceholder,
  emptyLabel,
  emptyState,
  createAction,
  filters,
  mobileBack,
  batch,
  children,
}: WorkspaceLayoutProps<T, Id>) {
  const [query, setQuery] = useState("");
  const pane = useWorkspacePaneWidth(storageKey);
  const filtered = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase();
    return needle
      ? items.filter((item) => getSearchText(item).toLocaleLowerCase().includes(needle))
      : items;
  }, [getSearchText, items, query]);
  const hasDetail = selectedId !== null;
  const filteredIds = filtered.map(getId);
  const allSelected = filteredIds.length > 0 && filteredIds.every((id) => batch?.selectedIds.has(id));

  return (
    <div className="flex min-h-[calc(100svh-3.5rem)]">
      <aside
        style={{ "--workspace-pane-width": `${pane.width}px` } as CSSProperties}
        className={cn(
          "w-full flex-col border-r bg-background md:sticky md:top-14 md:flex md:h-[calc(100svh-3.5rem)] md:w-[var(--workspace-pane-width)] md:shrink-0",
          hasDetail ? "hidden" : "flex",
        )}
      >
        <div className="grid gap-3 border-b p-3">
          <div className="flex items-center justify-between gap-2">
            <h1 className="truncate text-lg font-semibold">{title}</h1>
            <div className="flex items-center gap-1">
              {batch && (
                <Button variant="outline" size="sm" onClick={batch.active ? batch.onExit : batch.onEnter}>
                  {batch.active ? batch.cancelLabel : batch.entryLabel}
                </Button>
              )}
              {!batch?.active && createAction}
            </div>
          </div>
          <div className="relative">
            <Search className="pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" aria-hidden />
            <Input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder={searchPlaceholder}
              className="pl-8"
            />
          </div>
          {filters}
          {batch?.active && (
            <label className="flex items-center gap-2 text-xs text-muted-foreground">
              <input
                type="checkbox"
                checked={allSelected}
                onChange={() => batch.onToggleAll(filteredIds)}
              />
              {batch.selectAllLabel}
            </label>
          )}
        </div>

        <div className="flex-1 overflow-y-auto p-2">
          {filtered.length === 0 && <p className="p-3 text-sm text-muted-foreground">{emptyLabel}</p>}
          <ul className="grid gap-1">
            {filtered.map((item) => {
              const id = getId(item);
              const active = String(id) === String(selectedId);
              const content = (
                <div className="min-w-0">
                  <div className="truncate text-sm font-medium">{renderTitle(item)}</div>
                  <div className="mt-0.5 flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
                    {renderSummary(item)}
                  </div>
                </div>
              );
              return (
                <li
                  key={id}
                  className={cn(
                    "flex min-h-14 items-center gap-2 rounded-md border border-transparent",
                    active ? "bg-accent text-accent-foreground" : "hover:bg-muted/60",
                  )}
                >
                  {batch?.active ? (
                    <label className="flex min-w-0 flex-1 cursor-pointer items-center gap-2 px-3 py-2 text-left">
                      <input
                        type="checkbox"
                        checked={batch.selectedIds.has(id)}
                        onChange={() => batch.onToggle(id)}
                        className="shrink-0"
                      />
                      {content}
                    </label>
                  ) : (
                    renderLink(item, content, "min-w-0 flex-1 px-3 py-2")
                  )}
                  {!batch?.active && renderAction && <div className="shrink-0 pr-3">{renderAction(item)}</div>}
                </li>
              );
            })}
          </ul>
        </div>
        {batch?.active && batch.footer && <div className="border-t p-2">{batch.footer}</div>}
      </aside>

      <VerticalResizeHandle
        label={title}
        width={pane.width}
        minWidth={pane.minWidth}
        maxWidth={pane.maxWidth}
        onWidthChange={pane.setWidth}
        onReset={pane.resetWidth}
      />

      <section className={cn("min-w-0 flex-1", hasDetail ? "block" : "hidden md:block")}>
        {hasDetail ? (
          <>
            {mobileBack && <div className="border-b px-4 py-2 md:hidden">{mobileBack}</div>}
            {children}
          </>
        ) : emptyState}
      </section>
    </div>
  );
}

import { useId, useState, type AriaAttributes, type ReactNode } from "react"
import { useTranslation } from "react-i18next"
import { useQuery } from "@tanstack/react-query"
import { ErrorNotice } from "@/components/state"
import { Pagination } from "@/components/data-table"
import { usePagination } from "@/lib/use-pagination"
import { ChevronsUpDown } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from "@/components/ui/command"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"
import { cn } from "@/lib/utils"

export type SelectOption = { value: string; label: string }
export type OptionSource = { key: readonly unknown[]; list: (search: string, page: number, pageSize: number) => Promise<{ items: SelectOption[]; total: number }> }
type Props = Pick<AriaAttributes, "aria-describedby" | "aria-required" | "aria-invalid"> & {
  id: string
  label: string
  value: string
  options?: ReadonlyArray<SelectOption>
  source?: OptionSource
  disabled?: boolean
  allowCustom?: boolean
  wrap?: boolean
  emptyValue?: string
  emptyLabel?: string
  onChange: (value: string) => void
}
type RemoteState = { open: boolean; search: string; onOpen: (open: boolean) => void; onSearch: (search: string) => void; items: SelectOption[]; loading: boolean; footer: ReactNode }

export function SearchableSelect(props: Props) {
  return props.source ? <RemoteSelect {...props} source={props.source} /> : <SelectControl {...props} />
}

function RemoteSelect(props: Props & { source: OptionSource }) {
  const [open, setOpen] = useState(false), [search, setSearch] = useState("")
  const { page, pageSize, setPage, setPageSize } = usePagination("", JSON.stringify([props.source.key, search]))
  const remote = useQuery({ queryKey: [...props.source.key, "options", props.id, search, page, pageSize], queryFn: () => props.source.list(search, page, pageSize), enabled: open })
  return <SelectControl {...props} remote={{ open, search, onOpen: next => { setOpen(next); setSearch(""); setPage(1) }, onSearch: setSearch, items: remote.data?.items ?? [], loading: remote.isFetching,
    footer: <>{remote.error ? <ErrorNotice error={remote.error} /> : null}<div className="border-t p-2"><Pagination page={page} pageSize={pageSize} total={remote.data?.total ?? 0} onPage={setPage} onPageSize={setPageSize} /></div></>,
  }} />
}

function SelectControl({ id, label, value, options = [], allowCustom, wrap, emptyLabel, emptyValue = "", disabled, onChange, remote, ...aria }: Props & { remote?: RemoteState }) {
  const { t } = useTranslation()
  const listId = useId()
  const [localOpen, setLocalOpen] = useState(false), [localSearch, setLocalSearch] = useState("")
  const [chosen, setChosen] = useState<SelectOption | null>(null)
  const open = remote?.open ?? localOpen, search = remote?.search ?? localSearch
  const setOpen = remote?.onOpen ?? ((next: boolean) => { setLocalOpen(next); setLocalSearch("") })
  const setSearch = remote?.onSearch ?? setLocalSearch
  const visible = [...new Map((remote?.items ?? options).map(option => [option.value, option])).values()]
  const custom = search.trim()
  const select = (next: string) => { setChosen(visible.find(option => option.value === next) ?? null); onChange(next); setOpen(false) }
  const labelClass = cn("min-w-0", wrap ? "whitespace-normal wrap-anywhere text-left" : "truncate")
  return <Popover modal open={open} onOpenChange={setOpen}>
    <PopoverTrigger asChild>
      <Button id={id} disabled={disabled} type="button" variant="outline" role="combobox" aria-haspopup="dialog" aria-label={label} aria-expanded={open} aria-controls={open ? listId : undefined} aria-describedby={aria["aria-describedby"]} aria-required={aria["aria-required"]} aria-invalid={aria["aria-invalid"]} className={cn("w-full min-w-0 justify-between", wrap && "h-auto min-h-8 py-1.5")}>
        <span className={labelClass}>{(chosen?.value === value ? chosen.label : undefined) ?? options.find(option => option.value === value)?.label ?? visible.find(option => option.value === value)?.label ?? (value === emptyValue ? emptyLabel || t("form.choose") : value || emptyLabel || t("form.choose"))}</span>
        <ChevronsUpDown data-icon="inline-end" />
      </Button>
    </PopoverTrigger>
    <PopoverContent id={listId} aria-label={label} align="start" className="w-(--radix-popover-trigger-width) p-0">
      <Command shouldFilter={!remote}>
        <CommandInput aria-label={label} placeholder={t("actions.search")} value={search} onValueChange={setSearch} />
        <CommandList>
          <CommandEmpty>{t(remote?.loading ? "state.loading" : "state.emptyTitle")}</CommandEmpty>
          <CommandGroup>
            {emptyLabel ? <CommandItem value="__empty" keywords={[emptyLabel]} onSelect={() => select(emptyValue)} data-checked={value === emptyValue || !value}>{emptyLabel}</CommandItem> : null}
            {visible.map(option => <CommandItem key={option.value} value={option.value} keywords={[option.label]} onSelect={() => select(option.value)} data-checked={option.value === value}><span className={labelClass}>{option.label}</span></CommandItem>)}
            {allowCustom && custom && !visible.some(option => option.value === custom) ? <CommandItem value={custom} onSelect={() => select(custom)}><span className={labelClass}>{t("form.useCustom", { value: custom })}</span></CommandItem> : null}
          </CommandGroup>
        </CommandList>
      </Command>
      {remote?.footer}
    </PopoverContent>
  </Popover>
}

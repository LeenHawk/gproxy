import { useId, useState } from "react"
import { useTranslation } from "react-i18next"
import { ChevronsUpDown } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from "@/components/ui/command"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"

export function SearchableSelect({ id, label, value, options, allowCustom, emptyLabel, emptyValue = "", onChange }: {
  id: string
  label: string
  value: string
  options: ReadonlyArray<{ value: string; label: string }>
  allowCustom?: boolean
  emptyValue?: string
  emptyLabel?: string
  onChange: (value: string) => void
}) {
  const { t } = useTranslation()
  const listId = useId()
  const [open, setOpen] = useState(false)
  const [search, setSearch] = useState("")
  const custom = search.trim()
  const select = (next: string) => { onChange(next); setOpen(false) }
  return <Popover modal open={open} onOpenChange={next => { setOpen(next); setSearch("") }}>
    <PopoverTrigger asChild>
      <Button id={id} type="button" variant="outline" role="combobox" aria-label={label} aria-expanded={open} aria-controls={listId} className="w-full min-w-0 justify-between">
        <span className="truncate">{options.find(option => option.value === value)?.label ?? (value === emptyValue ? emptyLabel || t("form.choose") : value || emptyLabel || t("form.choose"))}</span>
        <ChevronsUpDown data-icon="inline-end" />
      </Button>
    </PopoverTrigger>
    <PopoverContent align="start" className="w-(--radix-popover-trigger-width) p-0">
      <Command>
        <CommandInput aria-label={label} placeholder={t("actions.search")} value={search} onValueChange={setSearch} />
        <CommandList id={listId}>
          <CommandEmpty>{t("state.emptyTitle")}</CommandEmpty>
          <CommandGroup>
            {emptyLabel ? <CommandItem value="__empty" keywords={[emptyLabel]} onSelect={() => select(emptyValue)} data-checked={value === emptyValue || !value}>{emptyLabel}</CommandItem> : null}
            {options.map(option => <CommandItem key={option.value} value={option.value} keywords={[option.label]} onSelect={() => select(option.value)} data-checked={option.value === value}><span className="truncate">{option.label}</span></CommandItem>)}
            {allowCustom && custom && !options.some(option => option.value === custom) ? <CommandItem value={custom} onSelect={() => select(custom)}>{t("form.useCustom", { value: custom })}</CommandItem> : null}
          </CommandGroup>
        </CommandList>
      </Command>
    </PopoverContent>
  </Popover>
}

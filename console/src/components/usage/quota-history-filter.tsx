import { useState } from "react"
import { useTranslation } from "react-i18next"
import { ChevronsUpDownIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Field, FieldGroup, FieldLabel, FieldLegend, FieldSet } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"

export function QuotaHistoryFilter({ label, options, excluded, onChange }: {
  label: string
  options: Array<{ value: string; label: string }>
  excluded: Set<string>
  onChange: (excluded: Set<string>) => void
}) {
  const { t } = useTranslation()
  const [search, setSearch] = useState("")
  const selected = options.filter((option) => !excluded.has(option.value)).length
  const visible = options.filter((option) => option.label.toLocaleLowerCase().includes(search.toLocaleLowerCase()))
  return <Popover>
    <PopoverTrigger asChild>
      <Button variant="outline" className="min-w-0 justify-between" aria-label={label}>
        <span className="truncate">{label} · {selected}/{options.length}</span><ChevronsUpDownIcon data-icon="inline-end" />
      </Button>
    </PopoverTrigger>
    <PopoverContent align="start" className="w-80 max-w-[calc(100vw-2rem)]">
      <Input aria-label={t("common.search")} placeholder={t("common.search")} value={search} onChange={(event) => setSearch(event.target.value)} />
      <div className="flex gap-2">
        <Button variant="ghost" size="sm" onClick={() => { const next = new Set(excluded); visible.forEach((option) => next.delete(option.value)); onChange(next) }}>{t("usage.quotaHistory.selectAll")}</Button>
        <Button variant="ghost" size="sm" onClick={() => onChange(new Set([...excluded, ...visible.map((option) => option.value)]))}>{t("usage.quotaHistory.selectNone")}</Button>
      </div>
      <FieldSet className="max-h-72 overflow-y-auto">
        <FieldLegend className="sr-only">{label}</FieldLegend>
        <FieldGroup className="gap-3">
          {visible.map((option) => <Field key={option.value} orientation="horizontal">
            <FieldLabel className="min-w-0 break-words">
              <Checkbox checked={!excluded.has(option.value)} onCheckedChange={(checked) => {
                const next = new Set(excluded)
                if (checked) next.delete(option.value)
                else next.add(option.value)
                onChange(next)
              }} />{option.label}
            </FieldLabel>
          </Field>)}
          {!visible.length && <p className="text-sm text-muted-foreground">{t("common.none")}</p>}
        </FieldGroup>
      </FieldSet>
    </PopoverContent>
  </Popover>
}

import { useTranslation } from "react-i18next"
import { Input } from "@/components/ui/input"
import { SearchableSelect } from "@/components/searchable-select"

export function ModelInput({ id, value, onChange, options, placeholder, required }: {
  id: string
  value: string
  onChange: (value: string) => void
  options: string[]
  placeholder?: string
  required?: boolean
}) {
  const { t } = useTranslation()
  return <div className="flex min-w-0 flex-col gap-2">
    <Input id={id} className="font-mono" value={value} required={required} placeholder={placeholder} onChange={(event) => onChange(event.target.value)} />
    {options.length > 0 ? <SearchableSelect value={value} options={options.map((model) => ({ value: model, label: model }))} placeholder={t("common.search")} searchPlaceholder={t("common.search")} emptyLabel={t("common.none")} ariaLabel={t("common.search")} onChange={onChange} /> : null}
  </div>
}

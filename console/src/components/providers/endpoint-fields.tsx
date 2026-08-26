import { PlusIcon, Trash2Icon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { SearchableSelect } from "@/components/searchable-select"
import { Button } from "@/components/ui/button"
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import type { EndpointRow } from "./settings-values"
import { humanizeSettingKey } from "./settings-values"

export function EndpointFields({
  kinds,
  rows,
  onChange,
}: {
  kinds: Array<string>
  rows: Array<EndpointRow>
  onChange: (rows: Array<EndpointRow>) => void
}) {
  const { t } = useTranslation()
  if (!kinds.length) return null
  const selected = new Set(rows.map((row) => row.kind))
  const options = kinds.map((kind) => ({
    value: kind,
    label: t(`providers.endpoints.kinds.${kind}`, { defaultValue: humanizeSettingKey(kind) }),
  }))
  const update = (index: number, patch: Partial<EndpointRow>) => {
    onChange(rows.map((row, rowIndex) => rowIndex === index ? { ...row, ...patch } : row))
  }
  return (
    <Field>
      <div className="flex items-center justify-between gap-3">
        <FieldLabel>{t("providers.endpoints.title")}</FieldLabel>
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={selected.size >= kinds.length}
          onClick={() => onChange([...rows, { kind: kinds.find((kind) => !selected.has(kind)) ?? "", url: "" }])}
        >
          <PlusIcon data-icon="inline-start" />
          {t("providers.endpoints.add")}
        </Button>
      </div>
      <FieldDescription>{t("providers.endpoints.hint")}</FieldDescription>
      <div className="flex flex-col gap-2">
        {rows.map((row, index) => (
          <div key={`${row.kind}-${index}`} className="grid gap-2 rounded-lg border p-2 sm:grid-cols-[minmax(0,14rem)_1fr_auto]">
            <SearchableSelect
              value={row.kind}
              options={options.filter((option) => option.value === row.kind || !selected.has(option.value))}
              placeholder={t("providers.endpoints.kind")}
              searchPlaceholder={t("common.search")}
              emptyLabel={t("common.none")}
              ariaLabel={t("providers.endpoints.kind")}
              onChange={(kind) => update(index, { kind })}
            />
            <Input
              type="url"
              className="font-mono"
              value={row.url}
              aria-label={t("providers.endpoints.url")}
              placeholder={t("providers.endpoints.urlPlaceholder")}
              onChange={(event) => update(index, { url: event.target.value })}
            />
            <Button
              type="button"
              size="icon-sm"
              variant="ghost"
              aria-label={t("providers.endpoints.remove")}
              onClick={() => onChange(rows.filter((_, rowIndex) => rowIndex !== index))}
            >
              <Trash2Icon />
            </Button>
          </div>
        ))}
      </div>
    </Field>
  )
}

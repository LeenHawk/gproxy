import { useId } from "react"
import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { api, query } from "@/api/client"
import type { Page, RoutingTargetDto } from "@/generated/sdk"
import { SearchableSelect } from "@/components/searchable-select"
import { Field, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"

const clients = [["OpenCode", "^user-agent: opencode/"], ["Claude CLI", "^user-agent: claude-cli/"], ["Codex", "^user-agent: codex"], ["Cursor", "(?i)\\bcursor\\b"]]
type OperationKey = { operation: string; dialect: string }
const key = (value: OperationKey) => JSON.stringify([value.operation, value.dialect])

export function FilterFields({ providerId, ruleSetId, model, onModel, operations, onOperations, headers, onHeaders }: {
  providerId?: string; ruleSetId: string
  model: string; onModel: (value: string) => void
  operations: string; onOperations: (value: string) => void
  headers: string; onHeaders: (value: string) => void
}) {
  const { t } = useTranslation(), id = useId()
  const routing = useQuery({ queryKey: ["operation-keys"], queryFn: () => api<RoutingTargetDto[]>("/admin/api/operation-keys") })
  const options = routing.data ?? []
  let selected: OperationKey[] | null
  try {
    const value: unknown = operations.trim() ? JSON.parse(operations) : []
    selected = Array.isArray(value) && value.every(v => v && typeof v.operation === "string" && typeof v.dialect === "string") ? value : null
  } catch { selected = null }
  return <>
    <Field data-field-span="full"><FieldLabel htmlFor={`${id}-model`}>{t("fields.filterModelPattern")}</FieldLabel>
      <SearchableSelect id={`${id}-model`} label={t("fields.filterModelPattern")} value={model} onChange={onModel} allowCustom wrap emptyLabel={t("rules.allModels")} source={{ key: ["model-names", providerId, ruleSetId], list: async (search, page, pageSize) => { const result = await api<Page<string>>(`/admin/api/model-names${query({ providerId, ruleSetId: providerId ? undefined : ruleSetId, search, page, pageSize })}`); return { items: result.items.map(value => ({ value, label: value })), total: result.total } } }} />
    </Field>
    <Field><FieldLabel htmlFor={`${id}-operations`}>{t("fields.filterOperationKeys")}</FieldLabel>
      <Textarea id={`${id}-operations`} value={operations} onChange={e => onOperations(e.target.value)} />
      <ToggleGroup type="multiple" variant="outline" size="sm" className="max-w-full flex-wrap" aria-label={t("fields.filterOperationKeys")} disabled={selected === null} value={selected?.map(key) ?? []} onValueChange={values => {
        const existing = selected ?? []
        const next = [...existing.filter(item => values.includes(key(item))), ...options.filter(item => values.includes(key(item)) && !existing.some(old => key(old) === key(item)))]
        onOperations(next.length ? JSON.stringify(next, null, 2) : "")
      }}>{options.map(item => <ToggleGroupItem key={key(item)} value={key(item)}>{t(`operation.${item.operation}`, { defaultValue: item.operation })} · {t(`rules.protocols.${item.dialect}`, { defaultValue: item.dialect })}</ToggleGroupItem>)}</ToggleGroup>
    </Field>
    <Field><FieldLabel htmlFor={`${id}-headers`}>{t("fields.filterHeaderPattern")}</FieldLabel>
      <Input id={`${id}-headers`} value={headers} onChange={e => onHeaders(e.target.value)} />
      <ToggleGroup type="single" variant="outline" size="sm" className="max-w-full flex-wrap" aria-label={t("fields.filterHeaderPattern")} value={headers} onValueChange={onHeaders}>{clients.map(([label, pattern]) => <ToggleGroupItem key={label} value={pattern}>{label}</ToggleGroupItem>)}</ToggleGroup>
    </Field>
  </>
}

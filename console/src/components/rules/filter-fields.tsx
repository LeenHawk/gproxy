import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { aliases, modelAliases, providerModels, routes } from "@/api/control"
import { channels } from "@/api/observability"
import { ModelInput } from "@/components/model-input"
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"

const CLIENTS = [
  ["OpenCode", "^user-agent: opencode/"],
  ["Claude CLI", "^user-agent: claude-cli/"],
  ["Codex", "^user-agent: codex"],
  ["Cursor", "(?i)\\bcursor\\b"],
]

export function FilterFields({ model, onModel, operations, onOperations, headers, onHeaders }: {
  model: string
  onModel: (value: string) => void
  operations: string
  onOperations: (value: string) => void
  headers: string
  onHeaders: (value: string) => void
}) {
  const { t } = useTranslation()
  const models = useQuery({ queryKey: ["provider-models"], queryFn: providerModels })
  const routeList = useQuery({ queryKey: ["routes"], queryFn: routes })
  const aliasList = useQuery({ queryKey: ["aliases"], queryFn: aliases })
  const modelAliasList = useQuery({ queryKey: ["model-aliases"], queryFn: modelAliases })
  const channelList = useQuery({ queryKey: ["channels"], queryFn: channels })
  const suggestions = [...new Set([
    ...(models.data ?? []).filter((item) => item.enabled).map((item) => item.model_id),
    ...(routeList.data ?? []).filter((item) => item.enabled).map((item) => item.name),
    ...(aliasList.data ?? []).filter((item) => item.enabled).map((item) => item.alias),
    ...(modelAliasList.data ?? []).filter((item) => item.enabled).map((item) => item.name),
  ])].sort()
  const operationOptions = [...new Set((channelList.data ?? []).flatMap((channel) => channel.supports.map((support) => support.operation)))].sort()
  const selected = operations.split(",").map((value) => value.trim()).filter(Boolean)
  return <>
    <Field>
      <FieldLabel htmlFor="rule-model-filter">{t("rules.filters.model")}</FieldLabel>
      <ModelInput id="rule-model-filter" value={model} onChange={onModel} options={suggestions} placeholder={t("rules.placeholders.allModels")} />
      <FieldDescription>{t("rules.filters.modelHelp")}</FieldDescription>
    </Field>
    <Field>
      <FieldLabel htmlFor="rule-operation-filter">{t("rules.filters.operations")}</FieldLabel>
      <Input id="rule-operation-filter" className="font-mono" value={operations} placeholder={t("rules.placeholders.allOperations")} onChange={(event) => onOperations(event.target.value)} />
      <ToggleGroup type="multiple" value={selected} onValueChange={(value) => onOperations(value.join(", "))} variant="outline" size="sm" className="max-w-full flex-wrap justify-start" aria-label={t("rules.filters.operations")}>
        {operationOptions.map((operation) => <ToggleGroupItem key={operation} value={operation} title={operation}>{t(`rules.operations.${operation}`, { defaultValue: operation })}</ToggleGroupItem>)}
      </ToggleGroup>
      <FieldDescription>{t("rules.filters.operationsHelp")}</FieldDescription>
    </Field>
    <Field>
      <FieldLabel htmlFor="rule-header-filter">{t("rules.filters.headers")}</FieldLabel>
      <Input id="rule-header-filter" className="font-mono" value={headers} placeholder={t("rules.placeholders.allHeaders")} onChange={(event) => onHeaders(event.target.value)} />
      <ToggleGroup type="single" value={headers} onValueChange={onHeaders} variant="outline" size="sm" className="max-w-full flex-wrap justify-start" aria-label={t("rules.filters.headers")}>
        {CLIENTS.map(([label, pattern]) => <ToggleGroupItem key={label} value={pattern} title={pattern}>{label}</ToggleGroupItem>)}
      </ToggleGroup>
      <FieldDescription>{t("rules.filters.headersHelp")}</FieldDescription>
    </Field>
  </>
}

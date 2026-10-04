import { useId } from "react"
import { ChevronDownIcon, PlusIcon, XIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import type { ModelMetadataDto } from "@/components/providers/provider-model-state"
import type { ModelReasoningLevelDto } from "@/components/providers/provider-model-state"
import type { ModelServiceTierDto } from "@/components/providers/provider-model-state"
import { Button } from "@/components/ui/button"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Textarea } from "@/components/ui/textarea"

export function ProviderModelMetadataFields({ value, onChange }: {
  value: ModelMetadataDto
  onChange: (value: ModelMetadataDto) => void
}) {
  const { t } = useTranslation()
  const id = useId()
  const set = <K extends keyof ModelMetadataDto>(key: K, next: ModelMetadataDto[K]) => onChange({ ...value, [key]: next })
  const optionalNumber = (input: string) => input ? Number(input) : null
  const truncationInvalid = value.truncation_policy != null && ((value.truncation_policy.mode !== "bytes" && value.truncation_policy.mode !== "tokens") || value.truncation_policy.limit == null)
  return <Collapsible data-field-span="full">
    <CollapsibleTrigger asChild>
      <Button type="button" variant="outline" className="group w-full justify-between">
        {t("providers.models.advancedMetadata")}
        <ChevronDownIcon data-icon="inline-end" className="transition-transform group-data-[state=open]:rotate-180" />
      </Button>
    </CollapsibleTrigger>
    <CollapsibleContent className="grid gap-4 pt-4 sm:grid-cols-2">
      <Field data-field-span="full">
        <FieldLabel htmlFor={`${id}-metadataDescription`}>{t("providers.models.metadataDescription")}</FieldLabel>
        <Input id={`${id}-metadataDescription`} value={value.description ?? ""} onChange={(event) => set("description", event.target.value || null)} />
      </Field>
      <Field>
        <FieldLabel htmlFor={`${id}-maxContextWindow`}>{t("providers.models.maxContextWindow")}</FieldLabel>
        <Input id={`${id}-maxContextWindow`} type="number" min="1" value={value.max_context_window ?? ""} onChange={(event) => set("max_context_window", optionalNumber(event.target.value))} />
      </Field>
      <Field>
        <FieldLabel htmlFor={`${id}-defaultReasoning`}>{t("providers.models.defaultReasoning")}</FieldLabel>
        <Input id={`${id}-defaultReasoning`} value={value.default_reasoning_level ?? ""} onChange={(event) => set("default_reasoning_level", event.target.value || null)} />
      </Field>
      <StringListField label={t("providers.models.inputModalities")} value={value.input_modalities} onChange={(next) => set("input_modalities", next)} />
      <StringListField label={t("providers.models.outputModalities")} value={value.output_modalities} onChange={(next) => set("output_modalities", next)} />
      <StringListField label={t("providers.models.supportedParameters")} value={value.supported_parameters} onChange={(next) => set("supported_parameters", next)} />
      <StringListField label={t("providers.models.generationMethods")} value={value.generation_methods} onChange={(next) => set("generation_methods", next)} />
      <StringListField label={t("providers.models.supportedActions")} value={value.supported_actions} onChange={(next) => set("supported_actions", next)} />
      <ReasoningLevels value={value.supported_reasoning_levels} onChange={(next) => set("supported_reasoning_levels", next)} />
      <ServiceTiers value={value.service_tiers} onChange={(next) => set("service_tiers", next)} />
      <Field>
        <FieldLabel htmlFor={`${id}-shellType`}>{t("providers.models.shellType")}</FieldLabel>
        <SuggestedInput id={`${id}-shellType`} value={value.shell_type} values={["unified_exec", "disabled", "default", "local", "shell_command"]} onChange={(next) => set("shell_type", next)} />
      </Field>
      <Field>
        <FieldLabel htmlFor={`${id}-defaultVerbosity`}>{t("providers.models.defaultVerbosity")}</FieldLabel>
        <SuggestedInput id={`${id}-defaultVerbosity`} value={value.default_verbosity} values={["low", "medium", "high"]} onChange={(next) => set("default_verbosity", next)} />
      </Field>
      <Field>
        <FieldLabel htmlFor={`${id}-defaultServiceTier`}>{t("providers.models.defaultServiceTier")}</FieldLabel>
        <Input id={`${id}-defaultServiceTier`} value={value.default_service_tier ?? ""} onChange={(event) => set("default_service_tier", event.target.value || null)} />
      </Field>
      <Field>
        <FieldLabel>{t("providers.models.reasoningSummary")}</FieldLabel>
        <div className="grid grid-cols-2 gap-2">
          <OptionalBoolean label={`${t("providers.models.reasoningSummary")}: ${t("form.supported")}`} value={value.supports_reasoning_summary_parameter} onChange={(next) => set("supports_reasoning_summary_parameter", next)} />
          <SuggestedInput label={`${t("providers.models.reasoningSummary")}: ${t("form.defaultValue")}`} value={value.default_reasoning_summary} values={["none", "auto", "concise", "detailed"]} onChange={(next) => set("default_reasoning_summary", next)} />
        </div>
      </Field>
      <Field>
        <FieldLabel htmlFor={`${id}-patchTool`}>{t("providers.models.patchTool")}</FieldLabel>
        <SuggestedInput id={`${id}-patchTool`} value={value.apply_patch_tool_type} values={["freeform"]} onChange={(next) => set("apply_patch_tool_type", next)} />
      </Field>
      <Field>
        <FieldLabel htmlFor={`${id}-webSearchTool`}>{t("providers.models.webSearchTool")}</FieldLabel>
        <SuggestedInput id={`${id}-webSearchTool`} value={value.web_search_tool_type} values={["text", "text_and_image"]} onChange={(next) => set("web_search_tool_type", next)} />
      </Field>
      <Field data-invalid={truncationInvalid}>
        <FieldLabel>{t("providers.models.truncation")}</FieldLabel>
        <div className="grid grid-cols-2 gap-2">
          <Select value={value.truncation_policy?.mode ?? "unknown"} onValueChange={(next) => set("truncation_policy", next === "unknown" ? null : { ...value.truncation_policy, mode: next as "bytes" | "tokens", limit: value.truncation_policy?.limit ?? null })}>
            <SelectTrigger aria-invalid={truncationInvalid} aria-label={`${t("providers.models.truncation")}: ${t("form.mode")}`}><SelectValue /></SelectTrigger>
            <SelectContent><SelectGroup><SelectItem value="unknown">{t("form.unset")}</SelectItem><SelectItem value="bytes">bytes</SelectItem><SelectItem value="tokens">tokens</SelectItem></SelectGroup></SelectContent>
          </Select>
          <Input aria-invalid={truncationInvalid} aria-label={`${t("providers.models.truncation")}: ${t("form.limit")}`} type="number" min="1" value={value.truncation_policy?.limit ?? ""} onChange={(event) => {
            const limit = optionalNumber(event.target.value)
            set("truncation_policy", limit == null && value.truncation_policy?.mode == null ? null : { ...value.truncation_policy, mode: value.truncation_policy?.mode ?? null, limit })
          }} />
        </div>
      </Field>
      <Field>
        <FieldLabel htmlFor={`${id}-searchSupport`}>{t("providers.models.searchSupport")}</FieldLabel>
        <OptionalBoolean id={`${id}-searchSupport`} value={value.supports_search_tool} onChange={(next) => set("supports_search_tool", next)} />
      </Field>
      <Field>
        <FieldLabel htmlFor={`${id}-autoCompactLimit`}>{t("providers.models.autoCompactLimit")}</FieldLabel>
        <Input id={`${id}-autoCompactLimit`} type="number" min="1" value={value.auto_compact_token_limit ?? ""} onChange={(event) => set("auto_compact_token_limit", optionalNumber(event.target.value))} />
      </Field>
      <Field>
        <FieldLabel htmlFor={`${id}-effectiveContextPercent`}>{t("providers.models.effectiveContextPercent")}</FieldLabel>
        <Input id={`${id}-effectiveContextPercent`} type="number" min="1" max="100" value={value.effective_context_window_percent ?? ""} onChange={(event) => set("effective_context_window_percent", optionalNumber(event.target.value))} />
      </Field>
      <div data-field-span="full" className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {([
          ["batch_supported", "batchSupport"],
          ["citations_supported", "citationsSupport"],
          ["code_execution_supported", "codeExecutionSupport"],
          ["context_management_supported", "contextManagementSupport"],
          ["structured_outputs_supported", "structuredOutputsSupport"],
          ["pdf_input_supported", "pdfInputSupport"],
          ["supports_image_detail_original", "imageDetailSupport"],
          ["support_verbosity", "verbositySupport"],
        ] as const).map(([field, label]) => <Field key={field}>
          <FieldLabel htmlFor={`${id}-${field}`}>{t(`providers.models.${label}`)}</FieldLabel>
          <OptionalBoolean id={`${id}-${field}`} value={value[field]} onChange={(next) => set(field, next)} />
        </Field>)}
      </div>
      <Field data-field-span="full">
        <FieldLabel htmlFor={`${id}-instructions`}>{t("providers.models.instructions")}</FieldLabel>
        <FieldDescription id={`${id}-instructions-help`}>{t("providers.models.instructionsHint")}</FieldDescription>
        <Textarea id={`${id}-instructions`} aria-describedby={`${id}-instructions-help`} className="min-h-40 font-mono text-xs" value={value.instructions ?? ""} onChange={(event) => set("instructions", event.target.value || null)} />
      </Field>
    </CollapsibleContent>
  </Collapsible>
}

function StringListField({ label, value, onChange }: { label: string; value: Array<string> | null; onChange: (value: Array<string> | null) => void }) {
  const { t } = useTranslation()
  return <Field data-field-span="full">
    <div className="flex items-center justify-between gap-2">
      <FieldLabel>{label}</FieldLabel>
      <div className="flex gap-1">
        <Button type="button" size="sm" variant="ghost" onClick={() => onChange(value == null ? [] : null)}>{t(value == null ? "providers.models.markKnown" : "providers.models.markUnknown")}</Button>
        <Button type="button" size="icon-sm" variant="ghost" disabled={value == null} onClick={() => onChange([...(value ?? []), ""])} aria-label={t("actions.add")}><PlusIcon data-icon="inline-start" /></Button>
      </div>
    </div>
    {value == null ? <FieldDescription>{t("providers.models.unknownMetadata")}</FieldDescription> : value.length === 0 ? <FieldDescription>{t("providers.models.knownEmpty")}</FieldDescription> : <div className="grid gap-2">{value.map((item, index) => <div key={index} className="flex gap-2">
      <Input aria-label={`${label}: ${t("providerForm.item", { index: index + 1 })}`} value={item} onChange={(event) => onChange(value.map((current, currentIndex) => currentIndex === index ? event.target.value : current))} />
      <Button type="button" size="icon-sm" variant="ghost" aria-label={t("actions.delete")} onClick={() => onChange(value.filter((_, currentIndex) => currentIndex !== index))}><XIcon data-icon="inline-start" /></Button>
    </div>)}</div>}
  </Field>
}

function ReasoningLevels({ value, onChange }: { value: Array<ModelReasoningLevelDto> | null; onChange: (value: Array<ModelReasoningLevelDto> | null) => void }) {
  const { t } = useTranslation()
  return <Field data-field-span="full"><FieldLabel>{t("providers.models.reasoningLevels")}</FieldLabel>
    <Button type="button" size="sm" variant="ghost" onClick={() => onChange(value == null ? [] : [...value, { effort: "medium", description: "" }])}>{value == null ? t("providers.models.markKnown") : t("actions.add")}</Button>
    {value?.map((level, index) => <div key={index} className="grid grid-cols-1 sm:grid-cols-[10rem_1fr_auto] gap-2">
      <SuggestedInput label={`${t("providers.models.reasoningLevels")}: ${t("providerForm.item", { index: index + 1 })}`} value={level.effort} values={["none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra", "persistent"]} onChange={(effort) => onChange(value.map((item, current) => current === index ? { ...item, effort: effort ?? "" } : item))} />
      <Input aria-label={`${t("form.description")}: ${level.effort}`} value={level.description} onChange={(event) => onChange(value.map((item, current) => current === index ? { ...item, description: event.target.value } : item))} />
      <Button type="button" size="icon-sm" variant="ghost" aria-label={t("actions.delete")} onClick={() => onChange(value.filter((_, current) => current !== index))}><XIcon data-icon="inline-start" /></Button>
    </div>)}
  </Field>
}

function ServiceTiers({ value, onChange }: { value: Array<ModelServiceTierDto> | null; onChange: (value: Array<ModelServiceTierDto> | null) => void }) {
  const { t } = useTranslation()
  return <Field data-field-span="full"><FieldLabel>{t("providers.models.serviceTiers")}</FieldLabel>
    <Button type="button" size="sm" variant="ghost" onClick={() => onChange(value == null ? [] : [...value, { id: "", name: "", description: "" }])}>{value == null ? t("providers.models.markKnown") : t("actions.add")}</Button>
    {value?.map((tier, index) => <div key={index} className="grid grid-cols-1 sm:grid-cols-2 gap-2">
      {(["id", "name", "description"] as const).map((field) => <Input key={field} aria-label={`${t("providers.models.serviceTiers")}: ${index + 1}, ${field === "description" ? t("form.description") : t(`fields.${field}`)}`} value={tier[field]} placeholder={field === "description" ? t("form.description") : t(`fields.${field}`)} onChange={(event) => onChange(value.map((item, current) => current === index ? { ...item, [field]: event.target.value } : item))} />)}
      <Button type="button" size="icon-sm" variant="ghost" aria-label={t("actions.delete")} onClick={() => onChange(value.filter((_, current) => current !== index))}><XIcon data-icon="inline-start" /></Button>
    </div>)}
  </Field>
}

function SuggestedInput({ id: inputId, label, value, values, onChange }: { id?: string; label?: string; value: string | null; values: Array<string>; onChange: (value: string | null) => void }) {
  const id = useId()
  const { t } = useTranslation()
  return <>
    <Input id={inputId} aria-label={label} list={id} value={value ?? ""} placeholder={t("form.unset")} onChange={(event) => onChange(event.target.value || null)} />
    <datalist id={id}>{values.map((item) => <option key={item} value={item} />)}</datalist>
  </>
}

function OptionalBoolean({ id, label, value, onChange }: { id?: string; label?: string; value: boolean | null; onChange: (value: boolean | null) => void }) {
  return <Select value={value == null ? "unknown" : String(value)} onValueChange={(next) => onChange(next === "unknown" ? null : next === "true")}><SelectTrigger id={id} aria-label={label}><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value="unknown">unknown</SelectItem><SelectItem value="true">true</SelectItem><SelectItem value="false">false</SelectItem></SelectGroup></SelectContent></Select>
}

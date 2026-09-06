import { ChevronDownIcon, PlusIcon, XIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import type { ModelMetadataDto } from "@/generated/ModelMetadataDto"
import type { ModelReasoningLevelDto } from "@/generated/ModelReasoningLevelDto"
import type { ModelServiceTierDto } from "@/generated/ModelServiceTierDto"
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
  const set = <K extends keyof ModelMetadataDto>(key: K, next: ModelMetadataDto[K]) => onChange({ ...value, [key]: next })
  const optionalNumber = (input: string) => input ? Number(input) : null
  return <Collapsible data-field-span="full">
    <CollapsibleTrigger asChild>
      <Button type="button" variant="outline" className="group w-full justify-between">
        {t("providers.models.advancedMetadata")}
        <ChevronDownIcon data-icon="inline-end" className="transition-transform group-data-[state=open]:rotate-180" />
      </Button>
    </CollapsibleTrigger>
    <CollapsibleContent className="grid gap-4 pt-4 sm:grid-cols-2">
      <Field data-field-span="full">
        <FieldLabel>{t("providers.models.metadataDescription")}</FieldLabel>
        <Input value={value.description ?? ""} onChange={(event) => set("description", event.target.value || null)} />
      </Field>
      <Field>
        <FieldLabel>{t("providers.models.maxContextWindow")}</FieldLabel>
        <Input type="number" min="1" value={value.max_context_window ?? ""} onChange={(event) => set("max_context_window", optionalNumber(event.target.value))} />
      </Field>
      <Field>
        <FieldLabel>{t("providers.models.defaultReasoning")}</FieldLabel>
        <Input value={value.default_reasoning_level ?? ""} onChange={(event) => set("default_reasoning_level", event.target.value || null)} />
      </Field>
      <StringListField label={t("providers.models.inputModalities")} value={value.input_modalities} onChange={(next) => set("input_modalities", next)} />
      <StringListField label={t("providers.models.outputModalities")} value={value.output_modalities} onChange={(next) => set("output_modalities", next)} />
      <StringListField label={t("providers.models.supportedParameters")} value={value.supported_parameters} onChange={(next) => set("supported_parameters", next)} />
      <StringListField label={t("providers.models.generationMethods")} value={value.generation_methods} onChange={(next) => set("generation_methods", next)} />
      <StringListField label={t("providers.models.supportedActions")} value={value.supported_actions} onChange={(next) => set("supported_actions", next)} />
      <ReasoningLevels value={value.reasoning_levels} onChange={(next) => set("reasoning_levels", next)} />
      <ServiceTiers value={value.service_tiers} onChange={(next) => set("service_tiers", next)} />
      <Field>
        <FieldLabel>{t("providers.models.shellType")}</FieldLabel>
        <OptionalSelect value={value.shell_type} values={["unified_exec", "disabled"]} onChange={(next) => set("shell_type", next)} />
      </Field>
      <Field>
        <FieldLabel>{t("providers.models.defaultVerbosity")}</FieldLabel>
        <OptionalSelect value={value.default_verbosity} values={["low", "medium", "high"]} onChange={(next) => set("default_verbosity", next)} />
      </Field>
      <Field>
        <FieldLabel>{t("providers.models.defaultServiceTier")}</FieldLabel>
        <Input value={value.default_service_tier ?? ""} onChange={(event) => set("default_service_tier", event.target.value || null)} />
      </Field>
      <Field>
        <FieldLabel>{t("providers.models.reasoningSummary")}</FieldLabel>
        <div className="grid grid-cols-2 gap-2">
          <OptionalBoolean value={value.supports_reasoning_summary_parameter} onChange={(next) => set("supports_reasoning_summary_parameter", next)} />
          <OptionalSelect value={value.default_reasoning_summary} values={["none", "auto", "concise", "detailed"]} onChange={(next) => set("default_reasoning_summary", next)} />
        </div>
      </Field>
      <Field>
        <FieldLabel>{t("providers.models.patchTool")}</FieldLabel>
        <OptionalSelect value={value.apply_patch_tool_type} values={["freeform"]} onChange={(next) => set("apply_patch_tool_type", next)} />
      </Field>
      <Field>
        <FieldLabel>{t("providers.models.webSearchTool")}</FieldLabel>
        <OptionalSelect value={value.web_search_tool_type} values={["text", "text_and_image"]} onChange={(next) => set("web_search_tool_type", next)} />
      </Field>
      <Field>
        <FieldLabel>{t("providers.models.truncation")}</FieldLabel>
        <div className="grid grid-cols-2 gap-2">
          <OptionalSelect value={value.truncation_mode} values={["bytes", "tokens"]} onChange={(next) => set("truncation_mode", next)} />
          <Input type="number" min="1" value={value.truncation_limit ?? ""} onChange={(event) => set("truncation_limit", optionalNumber(event.target.value))} />
        </div>
      </Field>
      <Field>
        <FieldLabel>{t("providers.models.searchSupport")}</FieldLabel>
        <OptionalBoolean value={value.supports_search_tool} onChange={(next) => set("supports_search_tool", next)} />
      </Field>
      <Field>
        <FieldLabel>{t("providers.models.autoCompactLimit")}</FieldLabel>
        <Input type="number" min="1" value={value.auto_compact_token_limit ?? ""} onChange={(event) => set("auto_compact_token_limit", optionalNumber(event.target.value))} />
      </Field>
      <Field>
        <FieldLabel>{t("providers.models.effectiveContextPercent")}</FieldLabel>
        <Input type="number" min="1" max="100" value={value.effective_context_window_percent ?? ""} onChange={(event) => set("effective_context_window_percent", optionalNumber(event.target.value))} />
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
          <FieldLabel>{t(`providers.models.${label}`)}</FieldLabel>
          <OptionalBoolean value={value[field]} onChange={(next) => set(field, next)} />
        </Field>)}
      </div>
      <Field data-field-span="full">
        <FieldLabel>{t("providers.models.instructions")}</FieldLabel>
        <FieldDescription>{t("providers.models.instructionsHint")}</FieldDescription>
        <Textarea className="min-h-40 font-mono text-xs" value={value.instructions ?? ""} onChange={(event) => set("instructions", event.target.value || null)} />
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
        <Button type="button" size="icon-sm" variant="ghost" disabled={value == null} onClick={() => onChange([...(value ?? []), ""])} aria-label={t("common.actions.add")}><PlusIcon data-icon="inline-start" /></Button>
      </div>
    </div>
    {value == null ? <FieldDescription>{t("providers.models.unknownMetadata")}</FieldDescription> : value.length === 0 ? <FieldDescription>{t("providers.models.knownEmpty")}</FieldDescription> : <div className="grid gap-2">{value.map((item, index) => <div key={index} className="flex gap-2">
      <Input value={item} onChange={(event) => onChange(value.map((current, currentIndex) => currentIndex === index ? event.target.value : current))} />
      <Button type="button" size="icon-sm" variant="ghost" aria-label={t("common.actions.delete")} onClick={() => onChange(value.filter((_, currentIndex) => currentIndex !== index))}><XIcon data-icon="inline-start" /></Button>
    </div>)}</div>}
  </Field>
}

function ReasoningLevels({ value, onChange }: { value: Array<ModelReasoningLevelDto> | null; onChange: (value: Array<ModelReasoningLevelDto> | null) => void }) {
  const { t } = useTranslation()
  return <Field data-field-span="full"><FieldLabel>{t("providers.models.reasoningLevels")}</FieldLabel>
    <Button type="button" size="sm" variant="ghost" onClick={() => onChange(value == null ? [] : [...value, { effort: "medium", description: "" }])}>{value == null ? t("providers.models.markKnown") : t("common.actions.add")}</Button>
    {value?.map((level, index) => <div key={index} className="grid grid-cols-[10rem_1fr_auto] gap-2">
      <Select value={level.effort} onValueChange={(effort) => onChange(value.map((item, current) => current === index ? { ...item, effort } : item))}><SelectTrigger><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{["none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra"].map((effort) => <SelectItem key={effort} value={effort}>{effort}</SelectItem>)}</SelectGroup></SelectContent></Select>
      <Input value={level.description} onChange={(event) => onChange(value.map((item, current) => current === index ? { ...item, description: event.target.value } : item))} />
      <Button type="button" size="icon-sm" variant="ghost" aria-label={t("common.actions.delete")} onClick={() => onChange(value.filter((_, current) => current !== index))}><XIcon data-icon="inline-start" /></Button>
    </div>)}
  </Field>
}

function ServiceTiers({ value, onChange }: { value: Array<ModelServiceTierDto> | null; onChange: (value: Array<ModelServiceTierDto> | null) => void }) {
  const { t } = useTranslation()
  return <Field data-field-span="full"><FieldLabel>{t("providers.models.serviceTiers")}</FieldLabel>
    <Button type="button" size="sm" variant="ghost" onClick={() => onChange(value == null ? [] : [...value, { id: "", name: "", description: "" }])}>{value == null ? t("providers.models.markKnown") : t("common.actions.add")}</Button>
    {value?.map((tier, index) => <div key={index} className="grid grid-cols-[9rem_11rem_1fr_auto] gap-2">
      {(["id", "name", "description"] as const).map((field) => <Input key={field} value={tier[field]} placeholder={field} onChange={(event) => onChange(value.map((item, current) => current === index ? { ...item, [field]: event.target.value } : item))} />)}
      <Button type="button" size="icon-sm" variant="ghost" aria-label={t("common.actions.delete")} onClick={() => onChange(value.filter((_, current) => current !== index))}><XIcon data-icon="inline-start" /></Button>
    </div>)}
  </Field>
}

function OptionalSelect({ value, values, onChange }: { value: string | null; values: Array<string>; onChange: (value: string | null) => void }) {
  return <Select value={value ?? "unknown"} onValueChange={(next) => onChange(next === "unknown" ? null : next)}><SelectTrigger><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value="unknown">unknown</SelectItem>{values.map((item) => <SelectItem key={item} value={item}>{item}</SelectItem>)}</SelectGroup></SelectContent></Select>
}

function OptionalBoolean({ value, onChange }: { value: boolean | null; onChange: (value: boolean | null) => void }) {
  return <Select value={value == null ? "unknown" : String(value)} onValueChange={(next) => onChange(next === "unknown" ? null : next === "true")}><SelectTrigger><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value="unknown">unknown</SelectItem><SelectItem value="true">true</SelectItem><SelectItem value="false">false</SelectItem></SelectGroup></SelectContent></Select>
}

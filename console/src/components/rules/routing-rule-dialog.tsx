import { useMemo, useState } from "react"
import { useTranslation } from "react-i18next"
import type { ChannelDto } from "@/generated/ChannelDto"
import type { RoutingImplementationDto } from "@/generated/RoutingImplementationDto"
import type { RoutingRuleDto } from "@/generated/RoutingRuleDto"
import type { RoutingRuleWriteRequest } from "@/generated/RoutingRuleWriteRequest"
import { SearchableSelect } from "@/components/searchable-select"
import { Button } from "@/components/ui/button"
import { Dialog, DialogBody, DialogClose, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"

const IMPLEMENTATIONS: Array<RoutingImplementationDto> = ["passthrough", "transform_to", "local", "unsupported"]

// A content route may change stream-ness alone: a non-stream client can be
// served by a forced streaming upstream (aggregated), and a streaming client
// by a buffered upstream (synthesized). The channel declares neither
// combination, so the sibling operation is offered alongside declared targets.
const STREAM_SIBLING: Record<string, string> = { generate_content: "stream_generate_content", stream_generate_content: "generate_content" }
const unique = (values: Array<string>) => [...new Set(values)]

export function RoutingRuleDialog({ open, onOpenChange, providerId, channel, rule, saving, onSave }: {
  open: boolean
  onOpenChange: (open: boolean) => void
  providerId: number
  channel?: ChannelDto
  rule?: RoutingRuleDto
  saving: boolean
  onSave: (value: RoutingRuleWriteRequest, id?: number) => Promise<void>
}) {
  const { t } = useTranslation()
  const [operation, setOperation] = useState(rule?.operation ?? "")
  const [kind, setKind] = useState(rule?.kind ?? "")
  const [implementation, setImplementation] = useState<RoutingImplementationDto>(rule?.implementation ?? "passthrough")
  const [destOperation, setDestOperation] = useState(rule?.dest_operation ?? "")
  const [destKind, setDestKind] = useState(rule?.dest_kind ?? "")
  const [sortOrder, setSortOrder] = useState(String(rule?.sort_order ?? 0))
  const [enabled, setEnabled] = useState(rule?.enabled ?? true)
  const supports = useMemo(() => channel?.supports ?? [], [channel])
  const operations = useMemo(() => unique(supports.map((support) => support.operation)), [supports])
  const kinds = useMemo(() => unique(supports.filter((support) => support.operation === operation).map((support) => support.source)), [operation, supports])
  const targets = useMemo(() => supports.filter((support) => support.source === kind && (support.operation === operation || support.operation === STREAM_SIBLING[operation])), [kind, operation, supports])
  const destOperations = useMemo(() => unique([...targets.map((target) => target.target_operation), ...(STREAM_SIBLING[operation] ? [operation, STREAM_SIBLING[operation]] : [])]), [operation, targets])
  const destKinds = useMemo(() => unique(targets.filter((target) => target.target_operation === destOperation || STREAM_SIBLING[target.target_operation] === destOperation).map((target) => target.target)), [destOperation, targets])
  const reset = () => {
    setOperation(rule?.operation ?? operations[0] ?? "")
    setKind(rule?.kind ?? "")
    setImplementation(rule?.implementation ?? "passthrough")
    setDestOperation(rule?.dest_operation ?? "")
    setDestKind(rule?.dest_kind ?? "")
    setSortOrder(String(rule?.sort_order ?? 0))
    setEnabled(rule?.enabled ?? true)
  }
  const submit = async (event: React.FormEvent) => {
    event.preventDefault()
    await onSave({ provider_id: providerId, operation, kind, implementation, dest_operation: implementation === "transform_to" ? destOperation : null, dest_kind: implementation === "transform_to" ? destKind : null, sort_order: Number(sortOrder), enabled }, rule?.id)
    onOpenChange(false)
  }
  const selectProps = { placeholder: t("common.none"), searchPlaceholder: t("common.search"), emptyLabel: t("common.none") }
  return <Dialog open={open} onOpenChange={(next) => { if (next) reset(); onOpenChange(next) }}><DialogContent showCloseButton={false}><form className="flex min-h-0 flex-1 flex-col" onSubmit={(event) => void submit(event)}><DialogHeader><DialogTitle>{t(rule ? "rules.routing.edit" : "rules.routing.add")}</DialogTitle></DialogHeader><DialogBody><FieldGroup>
    <Field><FieldLabel htmlFor="routing-operation">{t("rules.fields.operation")}</FieldLabel><SearchableSelect {...selectProps} id="routing-operation" ariaLabel={t("rules.fields.operation")} value={operation} options={operations.map((value) => ({ value, label: t(`rules.operations.${value}`, { defaultValue: value }) }))} onChange={(value) => { setOperation(value); setKind("") }} /></Field>
    <Field><FieldLabel htmlFor="routing-kind">{t("rules.fields.kind")}</FieldLabel><SearchableSelect {...selectProps} id="routing-kind" ariaLabel={t("rules.fields.kind")} value={kind} options={kinds.map((value) => ({ value, label: t(`rules.wires.${value}`, { defaultValue: value }) }))} onChange={setKind} /></Field>
    <Field><FieldLabel htmlFor="routing-implementation">{t("rules.fields.implementation")}</FieldLabel><Select name="routing-implementation" value={implementation} onValueChange={(value) => setImplementation(value as RoutingImplementationDto)}><SelectTrigger id="routing-implementation" className="w-full"><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{IMPLEMENTATIONS.map((value) => <SelectItem key={value} value={value}>{t(`rules.values.${value}`)}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
    {implementation === "transform_to" ? <><Field><FieldLabel htmlFor="routing-dest-operation">{t("rules.fields.destinationOperation")}</FieldLabel><SearchableSelect {...selectProps} id="routing-dest-operation" ariaLabel={t("rules.fields.destinationOperation")} value={destOperation} options={destOperations.map((value) => ({ value, label: t(`rules.operations.${value}`, { defaultValue: value }) }))} onChange={(value) => { setDestOperation(value); setDestKind("") }} /></Field><Field><FieldLabel htmlFor="routing-dest-kind">{t("rules.fields.destinationKind")}</FieldLabel><SearchableSelect {...selectProps} id="routing-dest-kind" ariaLabel={t("rules.fields.destinationKind")} value={destKind} options={destKinds.map((value) => ({ value, label: t(`rules.wires.${value}`, { defaultValue: value }) }))} onChange={setDestKind} /></Field></> : null}
    <Field><FieldLabel htmlFor="routing-order">{t("rules.fields.declaredOrder")}</FieldLabel><Input id="routing-order" type="number" value={sortOrder} onChange={(event) => setSortOrder(event.target.value)} /></Field><Field orientation="horizontal"><FieldLabel htmlFor="routing-enabled">{t("rules.fields.enabled")}</FieldLabel><Switch id="routing-enabled" name="routing-enabled" checked={enabled} onCheckedChange={setEnabled} /></Field>
  </FieldGroup></DialogBody><DialogFooter><DialogClose asChild><Button type="button" variant="outline">{t("common.actions.cancel")}</Button></DialogClose><Button type="submit" disabled={saving || !operation || !kind || (implementation === "transform_to" && (!destOperation || !destKind))}>{t(saving ? "common.actions.saving" : "common.actions.save")}</Button></DialogFooter></form></DialogContent></Dialog>
}

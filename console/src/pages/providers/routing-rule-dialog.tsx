import { useState } from "react"
import { useTranslation } from "react-i18next"
import type { OperationRoutingDto, RoutingMappingWrite } from "@/generated/sdk"
import { ErrorNotice } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Dialog, DialogBody, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"

type Props = { open: boolean; onOpenChange: (open: boolean) => void; row?: OperationRoutingDto; rows: OperationRoutingDto[]; onSubmit: (row: OperationRoutingDto, write: RoutingMappingWrite) => void; pending: boolean; error: unknown }
export function RoutingRuleDialog(props: Props) {
  const { t } = useTranslation()
  return <Dialog open={props.open} onOpenChange={(open) => { if (!props.pending) props.onOpenChange(open) }}><DialogContent className="sm:max-w-lg" closeLabel={t("actions.close")} aria-describedby={undefined}>
    <DialogHeader><DialogTitle>{t(props.row ? "edit.operation-rules" : "create.operation-rules")}</DialogTitle></DialogHeader>
    {props.open ? <RoutingRuleForm key={props.row ? `${props.row.operation}:${props.row.dialect}` : "new"} {...props} /> : null}
  </DialogContent></Dialog>
}
function RoutingRuleForm({ row, rows, onSubmit, pending, error }: Props) {
  const { t } = useTranslation()
  const initial = row ?? rows.find((r) => r.operation === "generate_content" && r.dialect === "openai_chat") ?? rows[0]
  const [source, setSource] = useState(initial)
  const [implementation, setImplementation] = useState(initial?.mapping.implementation ?? "passthrough")
  const [targetOperation, setTargetOperation] = useState(initial?.mapping.target?.operation ?? "")
  const [targetDialect, setTargetDialect] = useState(initial?.mapping.target?.dialect ?? "")
  const selectSource = (next: OperationRoutingDto) => {
    setSource(next); setImplementation(next.mapping.implementation)
    setTargetOperation(next.mapping.target?.operation ?? ""); setTargetDialect(next.mapping.target?.dialect ?? "")
  }
  if (!source) return null
  const targets = source.targets
  const validTarget = targets.some((v) => v.operation === targetOperation && v.dialect === targetDialect)
  const choice = (id: string, label: string, value: string, options: Array<{ value: string; label: string; disabled?: boolean }>, onChange: (v: string) => void, disabled = false) => <Field><FieldLabel htmlFor={id}>{label}</FieldLabel><Select value={value} disabled={pending || disabled} onValueChange={onChange}><SelectTrigger id={id}><SelectValue placeholder={t("form.choose")} /></SelectTrigger><SelectContent><SelectGroup>{options.map((o) => <SelectItem key={o.value} value={o.value} disabled={o.disabled}>{o.label}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
  return <form onSubmit={(e) => { e.preventDefault(); onSubmit(source, { implementation, target: implementation === "transform_to" ? { operation: targetOperation, dialect: targetDialect } : null }) }} className="flex min-h-0 flex-col">
    <DialogBody>{error ? <ErrorNotice error={error} /> : null}<FieldGroup className="sm:grid-cols-1">
      {choice("routing-operation", t("fields.operation"), source.operation, [...new Set(rows.map((r) => r.operation))].map((value) => ({ value, label: t(`operation.${value}`, { defaultValue: value }) })), (value) => selectSource(rows.find((r) => r.operation === value)!), !!row)}
      {choice("routing-source", t("rules.incoming"), source.dialect, rows.filter((r) => r.operation === source.operation).map((r) => ({ value: r.dialect, label: t(`rules.protocols.${r.dialect}`) })), (value) => selectSource(rows.find((r) => r.operation === source.operation && r.dialect === value)!), !!row)}
      {choice("routing-implementation", t("rules.behavior"), implementation, ["passthrough", "transform_to", "local", "unsupported"].map((value) => ({ value, label: t(`rules.implementations.${value}`), disabled: value === "local" ? !source.localAvailable : value === "transform_to" ? targets.length === 0 : false })), setImplementation)}
      {implementation === "transform_to" ? <>
        {choice("routing-target-operation", t("rules.targetOperation"), targetOperation, [...new Set(targets.map((r) => r.operation))].map((value) => ({ value, label: t(`operation.${value}`, { defaultValue: value }) })), (value) => { setTargetOperation(value); setTargetDialect("") })}
        {choice("routing-target-protocol", t("rules.targetProtocol"), targetDialect, targets.filter((r) => r.operation === targetOperation).map((r) => ({ value: r.dialect, label: t(`rules.protocols.${r.dialect}`) })), setTargetDialect)}
      </> : null}
    </FieldGroup></DialogBody>
    <DialogFooter><Button type="submit" disabled={pending || (implementation === "transform_to" && !validTarget)}>{t("actions.save")}</Button></DialogFooter>
  </form>
}

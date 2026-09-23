import { useState } from "react"
import { useTranslation } from "react-i18next"
import { ArrowDown, ArrowUp, Plus, Trash2 } from "lucide-react"
import type { OperationRuleDto } from "@/generated/sdk"
import { operationChoices } from "@/pages/providers/operation-options"
import { dialects } from "@/pages/providers/config-schema"
import { ErrorNotice } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Dialog, DialogBody, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"

type Props = { open: boolean; onOpenChange: (open: boolean) => void; original?: OperationRuleDto; defaults?: { operation: string; dialects: string[] }; onSubmit: (body: Record<string, unknown>) => void; pending: boolean; error: unknown }
export function RoutingRuleDialog(props: Props) {
  const { t } = useTranslation()
  return <Dialog open={props.open} onOpenChange={(open) => { if (!props.pending) props.onOpenChange(open) }}><DialogContent className="sm:max-w-lg" closeLabel={t("actions.close")} aria-describedby={undefined}>
    <DialogHeader><DialogTitle>{t(props.original || props.defaults ? "edit.operation-rules" : "create.operation-rules")}</DialogTitle></DialogHeader>
    {props.open ? <RoutingRuleForm key={props.original?.id ?? "new"} {...props} /> : null}
  </DialogContent></Dialog>
}
function RoutingRuleForm({ original, defaults, onSubmit, pending, error }: Props) {
  const { t } = useTranslation()
  const [operation, setOperation] = useState(original?.operation ?? defaults?.operation ?? "generate_content")
  const [targets, setTargets] = useState<string[]>(Array.isArray(original?.target) ? original.target as string[] : defaults?.dialects ?? [])
  const move = (index: number, offset: number) => setTargets((current) => {
    const next = [...current]
    ;[next[index], next[index + offset]] = [next[index + offset], next[index]]
    return next
  })
  return <form onSubmit={(e) => { e.preventDefault(); onSubmit({ operation, action: "dialects", target: targets }) }} className="flex min-h-0 flex-col">
    <DialogBody>
      {error ? <ErrorNotice error={error} /> : null}
      <FieldGroup className="sm:grid-cols-1">
        <Field><FieldLabel htmlFor="routing-operation">{t("fields.operation")}</FieldLabel>
          <Select value={operation} disabled={pending} onValueChange={setOperation}><SelectTrigger id="routing-operation"><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{operationChoices.map(({ value }) => <SelectItem key={value} value={value}>{t(`operation.${value}`, { defaultValue: value })}</SelectItem>)}</SelectGroup></SelectContent></Select>
        </Field>
        <Field><FieldLabel>{t("rules.dialects")}</FieldLabel>
          {targets.map((value, index) => <div key={index} className="flex items-center gap-1">
            <Select value={value} disabled={pending} onValueChange={(next) => setTargets((current) => current.map((v, i) => i === index ? next : v))}>
              <SelectTrigger aria-label={t("rules.protocolNumber", { index: index + 1 })} className="min-w-0 flex-1"><SelectValue placeholder={t("form.choose")} /></SelectTrigger>
              <SelectContent><SelectGroup>{dialects.map((d) => <SelectItem key={d} value={d} disabled={d !== value && targets.includes(d)}>{t(`providerOption.${d}`)}</SelectItem>)}</SelectGroup></SelectContent>
            </Select>
            <Button type="button" size="icon-sm" variant="ghost" disabled={pending || index === 0} aria-label={t("rules.moveUp")} onClick={() => move(index, -1)}><ArrowUp /></Button>
            <Button type="button" size="icon-sm" variant="ghost" disabled={pending || index === targets.length - 1} aria-label={t("rules.moveDown")} onClick={() => move(index, 1)}><ArrowDown /></Button>
            <Button type="button" size="icon-sm" variant="ghost" disabled={pending} aria-label={t("actions.remove")} onClick={() => setTargets((current) => current.filter((_, i) => i !== index))}><Trash2 /></Button>
          </div>)}
          <Button type="button" variant="outline" size="sm" className="self-start" disabled={pending || targets.length >= dialects.length} onClick={() => setTargets((current) => [...current, ""])}><Plus data-icon="inline-start" />{t("rules.addProtocol")}</Button>
        </Field>
      </FieldGroup>
    </DialogBody>
    <DialogFooter><Button type="submit" disabled={pending || !targets.length || targets.some((v) => !v)}>{t(original || defaults ? "actions.save" : "actions.create")}</Button></DialogFooter>
  </form>
}

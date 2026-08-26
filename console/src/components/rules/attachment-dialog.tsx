import type { ReactElement } from "react"
import { useState } from "react"
import { useTranslation } from "react-i18next"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { ProviderRuleSetWriteRequest } from "@/generated/ProviderRuleSetWriteRequest"
import type { RuleSetDto } from "@/generated/RuleSetDto"
import { SearchableSelect } from "@/components/searchable-select"
import { Button } from "@/components/ui/button"
import { Dialog, DialogBody, DialogClose, DialogContent, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"

export function AttachmentDialog({ providers, ruleSets, fixedProviderId, fixedRuleSetId, trigger, saving, onSave }: {
  providers: Array<ProviderDto>
  ruleSets: Array<RuleSetDto>
  fixedProviderId?: number
  fixedRuleSetId?: number
  trigger: ReactElement
  saving: boolean
  onSave: (value: ProviderRuleSetWriteRequest) => Promise<void>
}) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const [providerId, setProviderId] = useState(String(fixedProviderId ?? providers[0]?.id ?? ""))
  const [ruleSetId, setRuleSetId] = useState(String(fixedRuleSetId ?? ruleSets[0]?.id ?? ""))
  const [sortOrder, setSortOrder] = useState("0")
  const reset = () => {
    setProviderId(String(fixedProviderId ?? providers[0]?.id ?? ""))
    setRuleSetId(String(fixedRuleSetId ?? ruleSets[0]?.id ?? ""))
    setSortOrder("0")
  }
  const submit = async (event: React.FormEvent) => {
    event.preventDefault()
    await onSave({ provider_id: Number(providerId), rule_set_id: Number(ruleSetId), sort_order: Number(sortOrder), enabled: true })
    setOpen(false)
  }
  const selectProps = { placeholder: t("common.none"), searchPlaceholder: t("common.search"), emptyLabel: t("common.none") }
  return (
    <Dialog open={open} onOpenChange={(next) => { if (next) reset(); setOpen(next) }}>
      <DialogTrigger asChild>{trigger}</DialogTrigger>
      <DialogContent showCloseButton={false}>
        <form className="flex min-h-0 flex-1 flex-col" onSubmit={(event) => void submit(event)}>
          <DialogHeader><DialogTitle>{t("rules.attachments.add")}</DialogTitle></DialogHeader>
          <DialogBody><FieldGroup>
            <Field><FieldLabel htmlFor="attachment-provider">{t("rules.fields.provider")}</FieldLabel><SearchableSelect {...selectProps} id="attachment-provider" ariaLabel={t("rules.fields.provider")} disabled={fixedProviderId != null} value={providerId} options={providers.map((provider) => ({ value: String(provider.id), label: provider.name }))} onChange={setProviderId} /></Field>
            <Field><FieldLabel htmlFor="attachment-set">{t("rules.fields.ruleSet")}</FieldLabel><SearchableSelect {...selectProps} id="attachment-set" ariaLabel={t("rules.fields.ruleSet")} disabled={fixedRuleSetId != null} value={ruleSetId} options={ruleSets.map((ruleSet) => ({ value: String(ruleSet.id), label: ruleSet.name }))} onChange={setRuleSetId} /></Field>
            <Field><FieldLabel htmlFor="attachment-order">{t("rules.fields.declaredOrder")}</FieldLabel><Input id="attachment-order" type="number" value={sortOrder} onChange={(event) => setSortOrder(event.target.value)} /></Field>
          </FieldGroup></DialogBody>
          <DialogFooter><DialogClose asChild><Button type="button" variant="outline">{t("common.actions.cancel")}</Button></DialogClose><Button type="submit" disabled={saving || !providerId || !ruleSetId}>{t(saving ? "common.actions.saving" : "rules.attachments.attach")}</Button></DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

import type { ReactElement } from "react"
import type { PriceRuleDto } from "@/generated/PriceRuleDto"
import type { PriceRuleWriteRequest } from "@/generated/PriceRuleWriteRequest"
import type { ProviderDto } from "@/generated/ProviderDto"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useState } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { priceCatalog, savePriceRule } from "@/api/control"
import { SearchableSelect } from "@/components/searchable-select"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Dialog, DialogBody, DialogClose, DialogContent, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import { TierEditor } from "./tier-editor"
import { losesLongContextStep, serializeTiers, tierDrafts } from "./tier-values"

export function PriceRuleDialog({ rule, providers, trigger, fixedProviderId, initialPattern, lockedPattern = false }: {
  rule?: PriceRuleDto
  providers: Array<ProviderDto>
  trigger: ReactElement
  fixedProviderId?: number | null
  initialPattern?: string
  lockedPattern?: boolean
}) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const [open, setOpen] = useState(false)
  const [provider, setProvider] = useState(fixedProviderId === undefined ? rule?.provider_id == null ? "all" : String(rule.provider_id) : fixedProviderId == null ? "all" : String(fixedProviderId))
  const [pattern, setPattern] = useState(rule?.model_pattern ?? initialPattern ?? "")
  const [priority, setPriority] = useState(String(rule?.priority ?? 0))
  const [enabled, setEnabled] = useState(rule?.enabled ?? true)
  const [tiers, setTiers] = useState(() => tierDrafts(rule?.tiers))
  const catalog = useQuery({ queryKey: ["price-catalog"], queryFn: priceCatalog, enabled: open })
  const serviceTiers = catalog.data?.service_tiers ?? [...new Set(tiers.map((tier) => tier.serviceTier).filter(Boolean))]
  const mutation = useMutation({
    mutationFn: (value: PriceRuleWriteRequest) => savePriceRule(value, rule?.id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["price-rules"] })
      toast.success(t(rule ? "pricing.rules.updated" : "pricing.rules.created"))
      setOpen(false)
    },
    onError: () => toast.error(t("pricing.rules.saveError")),
  })
  const submit = (event: React.FormEvent) => {
    event.preventDefault()
    mutation.mutate({
      provider_id: fixedProviderId === undefined ? provider === "all" ? null : Number(provider) : fixedProviderId,
      model_pattern: pattern.trim(),
      tiers: tiers.length ? serializeTiers(tiers) : null,
      priority: Number(priority),
      enabled,
    })
  }
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>{trigger}</DialogTrigger>
      <DialogContent className="sm:max-w-4xl" showCloseButton={false}>
        <form className="flex min-h-0 flex-1 flex-col" onSubmit={submit}>
          <DialogHeader><DialogTitle>{t(rule ? "pricing.rules.edit" : "pricing.rules.add")}</DialogTitle></DialogHeader>
          <DialogBody><FieldGroup>
            <div className="grid gap-4 sm:grid-cols-2">
              {fixedProviderId === undefined ? <Field>
                <FieldLabel htmlFor="price-provider">{t("pricing.rules.provider")}</FieldLabel>
                <SearchableSelect
                  id="price-provider"
                  value={provider}
                  options={[{ value: "all", label: t("pricing.rules.allProviders") }, ...providers.map((item) => ({ value: String(item.id), label: item.name }))]}
                  placeholder={t("common.none")}
                  searchPlaceholder={t("common.search")}
                  emptyLabel={t("common.none")}
                  ariaLabel={t("pricing.rules.provider")}
                  onChange={setProvider}
                />
              </Field> : null}
              <Field><FieldLabel htmlFor="price-pattern">{t("pricing.rules.pattern")}</FieldLabel><Input id="price-pattern" className="font-mono" required readOnly={lockedPattern} value={pattern} onChange={(event) => setPattern(event.target.value)} /></Field>
              <Field><FieldLabel htmlFor="price-priority">{t("pricing.rules.priority")}</FieldLabel><Input id="price-priority" type="number" step={1} required value={priority} onChange={(event) => setPriority(event.target.value)} /></Field>
              <Field orientation="horizontal"><FieldLabel htmlFor="price-enabled">{t("pricing.rules.enabled")}</FieldLabel><Switch id="price-enabled" checked={enabled} onCheckedChange={setEnabled} /></Field>
            </div>
            <TierEditor rows={tiers} serviceTiers={serviceTiers} onChange={setTiers} />
            {losesLongContextStep(tiers) ? <Alert><AlertTitle>{t("pricing.warning.title")}</AlertTitle><AlertDescription>{t("pricing.warning.longContext")}</AlertDescription></Alert> : null}
          </FieldGroup></DialogBody>
          <DialogFooter>
            <DialogClose asChild><Button type="button" variant="outline">{t("common.actions.cancel")}</Button></DialogClose>
            <Button type="submit" disabled={mutation.isPending}>{t(mutation.isPending ? "common.actions.saving" : "common.actions.save")}</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

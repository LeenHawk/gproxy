import { useId, useState } from "react"
import { useMutation } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { saveAlias } from "@/api/control"
import type { AliasDto } from "@/generated/AliasDto"
import type { AliasWriteRequest } from "@/generated/AliasWriteRequest"
import type { ProviderDto } from "@/generated/ProviderDto"
import { Button } from "@/components/ui/button"
import { SearchableSelect } from "@/components/searchable-select"
import {
  Dialog,
  DialogBody,
  DialogClose,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import { FormDialogContent } from "@/components/routes/form-dialog-content"

export function RoutingAliasForm({
  alias,
  providers,
  opener,
  onOpenChange,
  onChanged,
  fixedProviderId,
}: {
  alias: AliasDto | null
  providers: Array<ProviderDto>
  opener: HTMLElement | null
  onOpenChange: (open: boolean) => void
  onChanged: () => void
  fixedProviderId?: number | null
}) {
  const { t } = useTranslation()
  const aliasId = useId()
  const targetId = useId()
  const providerId = useId()
  const priorityId = useId()
  const enabledId = useId()
  const [incoming, setIncoming] = useState(alias?.alias ?? "")
  const [target, setTarget] = useState(alias?.target ?? "")
  const [provider, setProvider] = useState(fixedProviderId === undefined ? alias?.provider_id == null ? "any" : String(alias.provider_id) : fixedProviderId == null ? "any" : String(fixedProviderId))
  const [priority, setPriority] = useState(String(alias?.priority ?? 0))
  const [enabled, setEnabled] = useState(alias?.enabled ?? true)
  const mutation = useMutation({
    mutationFn: (value: AliasWriteRequest) => saveAlias(value, alias?.id),
    onSuccess: () => {
      toast.success(t(alias ? "routes.routingAliases.updated" : "routes.routingAliases.created"))
      onChanged()
      onOpenChange(false)
    },
    onError: () => toast.error(t("routes.routingAliases.saveError")),
  })

  function submit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault()
    mutation.mutate({
      alias: incoming.trim(),
      target: target.trim(),
      provider_id: fixedProviderId === undefined ? provider === "any" ? null : Number(provider) : fixedProviderId,
      priority: Number(priority),
      enabled,
    })
  }

  return (
    <Dialog open onOpenChange={onOpenChange}>
      <FormDialogContent opener={opener}>
        <form className="flex min-h-0 flex-1 flex-col" onSubmit={submit}>
          <DialogHeader>
            <DialogTitle>{t(alias ? "common.actions.edit" : fixedProviderId === undefined ? "routes.routingAliases.add" : "routes.routingAliases.modelAdd")}</DialogTitle>
          </DialogHeader>
          <DialogBody><FieldGroup>
            <Field>
              <FieldLabel htmlFor={aliasId}>{t("routes.routingAliases.alias")}</FieldLabel>
              <Input id={aliasId} className="font-mono" value={incoming} required autoFocus onChange={(event) => setIncoming(event.target.value)} />
            </Field>
            <Field>
              <FieldLabel htmlFor={targetId}>{t("routes.routingAliases.target")}</FieldLabel>
              <Input id={targetId} className="font-mono" value={target} required onChange={(event) => setTarget(event.target.value)} />
            </Field>
            {fixedProviderId === undefined ? <Field>
              <FieldLabel htmlFor={providerId}>{t("routes.routingAliases.provider")}</FieldLabel>
              <SearchableSelect
                id={providerId}
                value={provider}
                options={[
                  { value: "any", label: t("routes.routingAliases.anyProvider") },
                  ...providers.map((item) => ({ value: String(item.id), label: item.name })),
                ]}
                placeholder={t("common.none")}
                searchPlaceholder={t("common.search")}
                emptyLabel={t("common.none")}
                ariaLabel={t("routes.routingAliases.provider")}
                onChange={setProvider}
              />
            </Field> : null}
            <Field>
              <FieldLabel htmlFor={priorityId}>{t("routes.routingAliases.priority")}</FieldLabel>
              <Input id={priorityId} type="number" step={1} value={priority} required onChange={(event) => setPriority(event.target.value)} />
            </Field>
            <Field orientation="horizontal">
              <FieldLabel htmlFor={enabledId}>{t("routes.routingAliases.enabled")}</FieldLabel>
              <Switch id={enabledId} checked={enabled} onCheckedChange={setEnabled} />
            </Field>
          </FieldGroup></DialogBody>
          <DialogFooter>
            <DialogClose asChild><Button type="button" variant="outline">{t("common.actions.cancel")}</Button></DialogClose>
            <Button type="submit" disabled={mutation.isPending}>
              {t(mutation.isPending ? "common.actions.saving" : "common.actions.save")}
            </Button>
          </DialogFooter>
        </form>
      </FormDialogContent>
    </Dialog>
  )
}

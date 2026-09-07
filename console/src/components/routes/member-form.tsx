import { MemberModelInput } from "./member-model-input"
import { useId, useState } from "react"
import { useMutation } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { saveRouteMember } from "@/api/control"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { RouteDto } from "@/generated/RouteDto"
import type { RouteMemberDto } from "@/generated/RouteMemberDto"
import type { RouteMemberWriteRequest } from "@/generated/RouteMemberWriteRequest"
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
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import { FormDialogContent } from "@/components/routes/form-dialog-content"

export function MemberForm({
  route,
  member,
  providers,
  opener,
  onOpenChange,
  onChanged,
}: {
  route: RouteDto
  member: RouteMemberDto | null
  providers: Array<ProviderDto>
  opener: HTMLElement | null
  onOpenChange: (open: boolean) => void
  onChanged: () => void
}) {
  const { t } = useTranslation()
  const providerIdField = useId()
  const modelId = useId()
  const tierId = useId()
  const weightId = useId()
  const enabledId = useId()
  const [providerId, setProviderId] = useState(member?.provider_id ?? providers[0]?.id ?? 0)
  const [model, setModel] = useState(member?.upstream_model ?? "")
  const [tier, setTier] = useState(String(member?.tier ?? 0))
  const [weight, setWeight] = useState(String(member?.weight ?? 100))
  const [enabled, setEnabled] = useState(member?.enabled ?? true)
  const mutation = useMutation({
    mutationFn: (value: RouteMemberWriteRequest) => saveRouteMember(value, member?.id),
    onSuccess: () => {
      toast.success(t(member ? "routes.members.updated" : "routes.members.created"))
      onChanged()
      onOpenChange(false)
    },
    onError: () => toast.error(t("routes.members.saveError")),
  })

  function submit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault()
    mutation.mutate({
      route_id: route.id,
      provider_id: providerId,
      upstream_model: model.trim(),
      tier: Number(tier),
      weight: Number(weight),
      enabled,
    })
  }

  return (
    <Dialog open onOpenChange={onOpenChange}>
      <FormDialogContent opener={opener}>
        <form className="flex min-h-0 flex-1 flex-col" onSubmit={submit}>
          <DialogHeader>
            <DialogTitle>{t(member ? "common.actions.edit" : "routes.members.add")}</DialogTitle>
          </DialogHeader>
          <DialogBody><FieldGroup>
            <Field>
              <FieldLabel htmlFor={providerIdField}>{t("routes.members.provider")}</FieldLabel>
              <SearchableSelect
                value={String(providerId)}
                id={providerIdField}
                options={providers.map((provider) => ({ value: String(provider.id), label: provider.name, keywords: provider.channel }))}
                placeholder={t("common.none")}
                searchPlaceholder={t("common.search")}
                emptyLabel={t("common.none")}
                ariaLabel={t("routes.members.provider")}
                onChange={(value) => setProviderId(Number(value))}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor={modelId}>{t("routes.members.model")}</FieldLabel>
              <MemberModelInput provider={providers.find((provider) => provider.id === providerId)} id={modelId} value={model} onChange={setModel} />
            </Field>
            <div className="grid gap-4 sm:grid-cols-2">
              <Field>
                <FieldLabel htmlFor={tierId}>{t("routes.members.tier")}</FieldLabel>
                <Input id={tierId} type="number" min={0} step={1} value={tier} required onChange={(event) => setTier(event.target.value)} />
                <FieldDescription>{t("routes.members.tierHint")}</FieldDescription>
              </Field>
              <Field>
                <FieldLabel htmlFor={weightId}>{t("routes.members.weight")}</FieldLabel>
                <Input id={weightId} type="number" min={1} step={1} value={weight} required onChange={(event) => setWeight(event.target.value)} />
                <FieldDescription>{t("routes.members.weightHint")}</FieldDescription>
              </Field>
            </div>
            <Field orientation="horizontal">
              <FieldLabel htmlFor={enabledId}>{t("routes.members.enabled")}</FieldLabel>
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

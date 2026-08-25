import { useId, useMemo, useState } from "react"
import { useMutation } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { saveRouteMember } from "@/api/control"
import type { CredentialDto } from "@/generated/CredentialDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { RouteDto } from "@/generated/RouteDto"
import type { RouteMemberDto } from "@/generated/RouteMemberDto"
import type { RouteMemberWriteRequest } from "@/generated/RouteMemberWriteRequest"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogClose,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import { FormDialogContent } from "@/components/routes/form-dialog-content"

export function MemberForm({
  route,
  member,
  providers,
  credentials,
  opener,
  onOpenChange,
  onChanged,
}: {
  route: RouteDto
  member: RouteMemberDto | null
  providers: Array<ProviderDto>
  credentials: Array<CredentialDto>
  opener: HTMLElement | null
  onOpenChange: (open: boolean) => void
  onChanged: () => void
}) {
  const { t } = useTranslation()
  const providerIdField = useId()
  const credentialIdField = useId()
  const modelId = useId()
  const priorityId = useId()
  const enabledId = useId()
  const [providerId, setProviderId] = useState(member?.provider_id ?? providers[0]?.id ?? 0)
  const [credentialId, setCredentialId] = useState(member?.credential_id == null ? "any" : String(member.credential_id))
  const [model, setModel] = useState(member?.upstream_model ?? "")
  const [priority, setPriority] = useState(String(member?.priority ?? 0))
  const [enabled, setEnabled] = useState(member?.enabled ?? true)
  const providerCredentials = useMemo(
    () => credentials.filter((credential) => credential.provider_id === providerId),
    [credentials, providerId],
  )
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
      credential_id: credentialId === "any" ? null : Number(credentialId),
      upstream_model: model.trim(),
      priority: Number(priority),
      enabled,
    })
  }

  return (
    <Dialog open onOpenChange={onOpenChange}>
      <FormDialogContent opener={opener}>
        <form className="flex flex-col gap-4" onSubmit={submit}>
          <DialogHeader>
            <DialogTitle>{t(member ? "common.actions.edit" : "routes.members.add")}</DialogTitle>
          </DialogHeader>
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor={providerIdField}>{t("routes.members.provider")}</FieldLabel>
              <Select
                value={String(providerId)}
                onValueChange={(value) => {
                  setProviderId(Number(value))
                  setCredentialId("any")
                }}
              >
                <SelectTrigger id={providerIdField}><SelectValue /></SelectTrigger>
                <SelectContent><SelectGroup>{providers.map((provider) => (
                  <SelectItem key={provider.id} value={String(provider.id)}>{provider.name}</SelectItem>
                ))}</SelectGroup></SelectContent>
              </Select>
            </Field>
            <Field>
              <FieldLabel htmlFor={credentialIdField}>{t("routes.members.credential")}</FieldLabel>
              <Select value={credentialId} onValueChange={setCredentialId}>
                <SelectTrigger id={credentialIdField}><SelectValue /></SelectTrigger>
                <SelectContent><SelectGroup>
                  <SelectItem value="any">{t("routes.members.anyCredential")}</SelectItem>
                  {providerCredentials.map((credential) => (
                    <SelectItem key={credential.id} value={String(credential.id)}>
                      {credential.label ?? String(credential.id)}
                    </SelectItem>
                  ))}
                </SelectGroup></SelectContent>
              </Select>
            </Field>
            <Field>
              <FieldLabel htmlFor={modelId}>{t("routes.members.model")}</FieldLabel>
              <Input id={modelId} className="font-mono" value={model} required onChange={(event) => setModel(event.target.value)} />
            </Field>
            <Field>
              <FieldLabel htmlFor={priorityId}>{t("routes.members.priority")}</FieldLabel>
              <Input id={priorityId} type="number" step={1} value={priority} required onChange={(event) => setPriority(event.target.value)} />
              <FieldDescription>{t("routes.members.priorityHint")}</FieldDescription>
            </Field>
            <Field orientation="horizontal">
              <FieldLabel htmlFor={enabledId}>{t("routes.members.enabled")}</FieldLabel>
              <Switch id={enabledId} checked={enabled} onCheckedChange={setEnabled} />
            </Field>
          </FieldGroup>
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

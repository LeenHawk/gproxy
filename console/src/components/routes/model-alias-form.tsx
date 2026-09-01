import { useId, useState } from "react"
import { useMutation } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { saveModelAlias } from "@/api/control"
import type { ModelAliasDto } from "@/generated/ModelAliasDto"
import type { ModelAliasWriteRequest } from "@/generated/ModelAliasWriteRequest"
import type { RouteDto } from "@/generated/RouteDto"
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

export function ModelAliasForm({
  alias,
  routes,
  opener,
  onOpenChange,
  onChanged,
  fixedRouteId,
}: {
  alias: ModelAliasDto | null
  routes: Array<RouteDto>
  opener: HTMLElement | null
  onOpenChange: (open: boolean) => void
  onChanged: () => void
  fixedRouteId?: number
}) {
  const { t } = useTranslation()
  const nameId = useId()
  const routeId = useId()
  const enabledId = useId()
  const [name, setName] = useState(alias?.name ?? "")
  const [selectedRoute, setSelectedRoute] = useState(String(fixedRouteId ?? alias?.route_id ?? routes[0]?.id ?? 0))
  const [enabled, setEnabled] = useState(alias?.enabled ?? true)
  const mutation = useMutation({
    mutationFn: (value: ModelAliasWriteRequest) => saveModelAlias(value, alias?.id),
    onSuccess: () => {
      toast.success(t(alias ? "routes.aliases.updated" : "routes.aliases.created"))
      onChanged()
      onOpenChange(false)
    },
    onError: () => toast.error(t("routes.aliases.saveError")),
  })

  function submit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault()
    try {
      mutation.mutate({ name: name.trim(), route_id: Number(selectedRoute), enabled })
    } catch {
      toast.error(t("routes.aliases.invalidMetadata"))
    }
  }

  return (
    <Dialog open onOpenChange={onOpenChange}>
      <FormDialogContent opener={opener}>
        <form className="flex min-h-0 flex-1 flex-col" onSubmit={submit}>
          <DialogHeader>
            <DialogTitle>{t(alias ? "common.actions.edit" : "routes.aliases.add")}</DialogTitle>
          </DialogHeader>
          <DialogBody><FieldGroup>
            <Field>
              <FieldLabel htmlFor={nameId}>{t("routes.aliases.name")}</FieldLabel>
              <Input id={nameId} className="font-mono" value={name} required autoFocus onChange={(event) => setName(event.target.value)} />
            </Field>
            {fixedRouteId == null ? <Field>
              <FieldLabel htmlFor={routeId}>{t("routes.aliases.route")}</FieldLabel>
              <SearchableSelect
                id={routeId}
                value={selectedRoute}
                options={routes.map((route) => ({ value: String(route.id), label: route.name }))}
                placeholder={t("common.none")}
                searchPlaceholder={t("common.search")}
                emptyLabel={t("common.none")}
                ariaLabel={t("routes.aliases.route")}
                onChange={setSelectedRoute}
              />
            </Field> : null}
            <Field orientation="horizontal">
              <FieldLabel htmlFor={enabledId}>{t("routes.aliases.enabled")}</FieldLabel>
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

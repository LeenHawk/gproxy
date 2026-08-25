import type { CredentialDto } from "@/generated/CredentialDto"
import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import type { CredentialWriteRequest } from "@/generated/CredentialWriteRequest"
import { PencilIcon } from "lucide-react"
import { useId } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { CredentialCycleList } from "@/components/providers/credential-cycle-list"
import { CredentialDialog } from "@/components/providers/credential-dialog"
import { StatusBadge } from "@/components/status-badge"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Field, FieldLabel } from "@/components/ui/field"
import { Switch } from "@/components/ui/switch"
import { formatInstant } from "@/lib/format"

type Props = {
  credential: CredentialDto
  cycles: Array<CredentialQuotaCycleDto>
  cyclesLoading: boolean
  cyclesError: boolean
  saving: boolean
  onSave: (value: CredentialWriteRequest, id?: number) => Promise<void>
}

export function CredentialCard(props: Props) {
  const { t, i18n } = useTranslation()
  const id = useId()
  const credential = props.credential
  const name = credential.label ?? t("providers.credentials.unnamed", { id: credential.id })
  const observed = formatInstant(credential.health_observed_at, i18n.language)

  const setEnabled = async (enabled: boolean) => {
    try {
      await props.onSave({
        provider_id: credential.provider_id,
        label: credential.label,
        secret: null,
        enabled,
      }, credential.id)
      toast.success(t("providers.credentials.updated"))
    } catch {
      toast.error(t("providers.credentials.updateError"))
    }
  }

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle headingLevel={3} className="machine-text">{name}</CardTitle>
        <CardDescription className="machine-text">
          {t("providers.credentials.unnamed", { id: credential.id })}
        </CardDescription>
        <CardAction className="flex flex-wrap items-center justify-end gap-2">
          <StatusBadge status={credential.health} />
          {credential.health_response_status != null ? (
            <Badge variant="outline" className="machine-text" aria-label={t("providers.credentials.healthDetail")}>
              {credential.health_response_status}
            </Badge>
          ) : null}
          <Field orientation="horizontal" className="w-auto">
            <FieldLabel htmlFor={`${id}-enabled`} className="sr-only">{t("providers.credentials.enabled")}</FieldLabel>
            <Switch
              id={`${id}-enabled`}
              size="sm"
              checked={credential.enabled}
              onCheckedChange={(value) => void setEnabled(value)}
              disabled={props.saving}
            />
          </Field>
          <CredentialDialog
            providerId={credential.provider_id}
            credential={credential}
            onSave={props.onSave}
            trigger={<Button variant="outline" size="sm" aria-label={`${t("common.actions.edit")}: ${name}`}><PencilIcon data-icon="inline-start" />{t("common.actions.edit")}</Button>}
          />
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <p className="text-sm text-muted-foreground">
          {observed ? t("providers.credentials.healthObserved", { time: observed }) : t("providers.credentials.healthUnobserved")}
        </p>
        {credential.health_detail ? <p className="machine-text text-sm text-muted-foreground">{credential.health_detail}</p> : null}
        <CredentialCycleList cycles={props.cycles} loading={props.cyclesLoading} error={props.cyclesError} />
      </CardContent>
    </Card>
  )
}

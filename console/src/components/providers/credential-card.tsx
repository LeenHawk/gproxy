import type { ChannelDto } from "@/generated/ChannelDto"
import type { CredentialDto } from "@/generated/CredentialDto"
import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import type { CredentialWriteRequest } from "@/generated/CredentialWriteRequest"
import type { TlsPresetDto } from "@/generated/TlsPresetDto"
import { GaugeIcon } from "lucide-react"
import { useId, useMemo } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { CredentialCycleList } from "@/components/providers/credential-cycle-list"
import { CredentialModelHealth } from "@/components/providers/credential-model-health"
import { CredentialDialog } from "@/components/providers/credential-dialog"
import { EntityDeleteButton } from "@/components/entity-delete-button"
import { StatusBadge } from "@/components/status-badge"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { Field, FieldLabel } from "@/components/ui/field"
import { Switch } from "@/components/ui/switch"
import { formatInstant } from "@/lib/format"

type Props = {
  credential: CredentialDto
  channel?: ChannelDto
  presets: Array<TlsPresetDto>
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
  const cycleCount = useMemo(() => new Set(props.cycles.map((cycle) => cycle.window_key)).size, [props.cycles])

  const setEnabled = async (enabled: boolean) => {
    try {
      await props.onSave({
        provider_id: credential.provider_id,
        label: credential.label,
        kind: credential.kind,
        secret: null,
        enabled,
        weight: credential.weight,
        rpm_limit: credential.rpm_limit,
        tpm_limit: credential.tpm_limit,
        proxy_url: credential.proxy_url,
        tls_fingerprint: credential.tls_fingerprint,
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
          {t(`providers.credentials.kinds.${credential.kind}`, { defaultValue: credential.kind })} · #{credential.id}
        </CardDescription>
        <CardAction className="flex flex-wrap items-center justify-end gap-2">
          {/* Health is a badge; the observation time rides on it rather than costing a line of prose. */}
          <StatusBadge status={credential.health} title={observed ? t("providers.credentials.healthObserved", { time: observed }) : undefined} />
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
            channel={props.channel}
            presets={props.presets}
            onSave={props.onSave}
            trigger={<Button variant="outline" size="sm" aria-label={`${t("common.actions.edit")}: ${name}`}>{t("common.actions.edit")}</Button>}
          />
          <EntityDeleteButton entity="credentials" id={credential.id} label={name} queryKeys={["credentials", "credential-cycles"]} />
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {credential.health_detail ? <p className="machine-text text-sm text-muted-foreground">{credential.health_detail}</p> : null}
        <CredentialModelHealth values={credential.model_health} />
        {cycleCount > 0 ? (
          <Collapsible>
            <CollapsibleTrigger asChild>
              <Button variant="outline" size="sm" className="self-start">
                <GaugeIcon aria-hidden />
                {t("usage.credentialCycles")}
                <Badge variant="secondary">{cycleCount}</Badge>
              </Button>
            </CollapsibleTrigger>
            <CollapsibleContent className="pt-4">
              <CredentialCycleList cycles={props.cycles} loading={props.cyclesLoading} error={props.cyclesError} />
            </CollapsibleContent>
          </Collapsible>
        ) : null}
      </CardContent>
    </Card>
  )
}

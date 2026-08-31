import type { ChannelDto } from "@/generated/ChannelDto"
import type { CredentialDto } from "@/generated/CredentialDto"
import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import type { CredentialWriteRequest } from "@/generated/CredentialWriteRequest"
import type { QuotaProbeResponse } from "@/generated/QuotaProbeResponse"
import type { TlsPresetDto } from "@/generated/TlsPresetDto"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { GaugeIcon, RefreshCwIcon, RotateCcwIcon } from "lucide-react"
import { useId, useMemo, useState } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { ApiError } from "@/api/client"
import { probeCredentialQuota, resetCredentialQuota } from "@/api/control"
import { ConfirmDangerous } from "@/components/confirm-dangerous"
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
  const client = useQueryClient()
  const [quota, setQuota] = useState<QuotaProbeResponse | null>(null)
  const [resetOpen, setResetOpen] = useState(false)
  const probe = useMutation({
    mutationFn: () => probeCredentialQuota(credential.id),
    onSuccess: async (result) => {
      setQuota(result)
      await client.invalidateQueries({ queryKey: ["credential-cycles"] })
      toast.success(t("providers.credentials.quotaProbe.success", { count: result.windows.length }))
    },
    onError: (error) => toast.error(error instanceof ApiError ? error.message : t("providers.credentials.quotaProbe.error")),
  })
  const reset = useMutation({
    mutationFn: () => resetCredentialQuota(credential.id),
    onSuccess: async (result) => {
      setResetOpen(false)
      toast.success(t(`providers.credentials.quotaReset.outcomes.${result.outcome}`, { count: result.windows_reset ?? 0 }))
      try {
        const refreshed = await probeCredentialQuota(credential.id)
        setQuota(refreshed)
        await client.invalidateQueries({ queryKey: ["credential-cycles"] })
      } catch (error) {
        toast.error(error instanceof ApiError ? error.message : t("providers.credentials.quotaProbe.error"))
      }
    },
    onError: (error) => toast.error(error instanceof ApiError ? error.message : t("providers.credentials.quotaReset.error")),
  })
  const resetCredits = quota?.reset_credits

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
    <>
      <Card size="sm">
      <CardHeader>
        <CardTitle headingLevel={3} className="machine-text">{name}</CardTitle>
        <CardDescription className="machine-text">
          {t(`providers.credentials.kinds.${credential.kind}`, { defaultValue: credential.kind })} · #{credential.id}
        </CardDescription>
        <CardAction className="flex flex-wrap items-center justify-end gap-2">
          {/* Health is a badge; the observation time rides on it rather than costing a line of prose. */}
          <StatusBadge status={credential.health} title={observed ? t("providers.credentials.healthObserved", { time: observed }) : undefined} />
          <CredentialModelHealth values={credential.model_health} />
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
        {/* The control is always here — v2 shows a quota button whether or not a cycle has been observed. */}
        <Collapsible>
          <div className="flex flex-wrap items-center gap-2">
            <CollapsibleTrigger asChild>
              <Button variant="outline" size="sm">
                <GaugeIcon aria-hidden />
                {t("usage.credentialCycles")}
                {cycleCount > 0 ? <Badge variant="secondary">{cycleCount}</Badge> : null}
              </Button>
            </CollapsibleTrigger>
            {/* Live upstream call; some upstreams rate-limit it, so it only fires on demand. */}
            <Button variant="ghost" size="sm" disabled={probe.isPending || reset.isPending} onClick={() => probe.mutate()}>
              <RefreshCwIcon aria-hidden className={probe.isPending ? "animate-spin" : undefined} />
              {t("providers.credentials.quotaProbe.action")}
            </Button>
            {resetCredits ? (
              <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                <Badge variant="secondary">
                  {t("providers.credentials.quotaReset.count", { count: resetCredits.available_count })}
                </Badge>
                {resetCredits.expires_at != null ? (
                  <span>{t("providers.credentials.quotaReset.expires", { time: formatInstant(resetCredits.expires_at, i18n.language) })}</span>
                ) : null}
                <Button
                  variant="outline"
                  size="sm"
                  disabled={resetCredits.available_count === 0 || reset.isPending || probe.isPending}
                  onClick={() => setResetOpen(true)}
                >
                  <RotateCcwIcon aria-hidden />
                  {t("providers.credentials.quotaReset.action")}
                </Button>
              </div>
            ) : null}
          </div>
          <CollapsibleContent className="pt-4">
            <CredentialCycleList cycles={props.cycles} loading={props.cyclesLoading} error={props.cyclesError} />
          </CollapsibleContent>
        </Collapsible>
      </CardContent>
      </Card>
      <ConfirmDangerous
        open={resetOpen}
        onOpenChange={setResetOpen}
        title={t("providers.credentials.quotaReset.confirmTitle")}
        description={t("providers.credentials.quotaReset.confirmDescription")}
        confirmLabel={t("providers.credentials.quotaReset.confirmAction")}
        pending={reset.isPending}
        onConfirm={() => reset.mutate()}
      />
    </>
  )
}

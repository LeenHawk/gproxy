import type { ChannelDto } from "@/generated/ChannelDto"
import type { CredentialDto } from "@/generated/CredentialDto"
import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import type { CredentialWriteRequest } from "@/generated/CredentialWriteRequest"
import type { TlsPresetDto } from "@/generated/TlsPresetDto"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { ChevronDownIcon, ChevronsUpDownIcon, PencilIcon, RefreshCwIcon, RotateCcwIcon } from "lucide-react"
import { useId, useMemo, useState } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { ApiError } from "@/api/client"
import { probeCredentialQuota, resetCredentialQuota } from "@/api/control"
import { ConfirmDangerous } from "@/components/confirm-dangerous"
import { CredentialCycleList } from "@/components/providers/credential-cycle-list"
import { CredentialHealthBadge } from "@/components/providers/credential-model-health"
import { CredentialDialog } from "@/components/providers/credential-dialog"
import { EntityDeleteButton } from "@/components/entity-delete-button"
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
  const client = useQueryClient()
  const [resetOpen, setResetOpen] = useState(false)
  const [quotaOpen, setQuotaOpen] = useState(false)
  /* The probe result lives in the query cache, not component state — v2 kept
     it with an infinite staleTime so leaving and revisiting the credential
     does not discard the last observed snapshot. */
  const probe = useQuery({
    queryKey: ["credential-quota-probe", credential.id],
    queryFn: () => probeCredentialQuota(credential.id),
    enabled: false,
    retry: false,
    staleTime: Infinity,
    gcTime: Infinity,
  })
  const quota = probe.data ?? null
  const refresh = async () => {
    const result = await probe.refetch()
    if (result.isSuccess) {
      await client.invalidateQueries({ queryKey: ["credential-cycles"] })
      toast.success(t("providers.credentials.quotaProbe.success", { count: result.data.windows.length }))
    } else if (result.isError) {
      toast.error(result.error instanceof ApiError ? result.error.message : t("providers.credentials.quotaProbe.error"))
    }
  }
  const reset = useMutation({
    mutationFn: () => resetCredentialQuota(credential.id),
    onSuccess: async (result) => {
      setResetOpen(false)
      toast.success(t(`providers.credentials.quotaReset.outcomes.${result.outcome}`, { count: result.windows_reset ?? 0 }))
      await refresh()
    },
    onError: (error) => toast.error(error instanceof ApiError ? error.message : t("providers.credentials.quotaReset.error")),
  })
  const resetCredits = quota?.reset_credits
  const raw = useMemo(() => {
    if (!quota?.raw) return null
    try {
      return JSON.stringify(JSON.parse(quota.raw), null, 2)
    } catch {
      return quota.raw
    }
  }, [quota])

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
        {/* Status is information, not an action — it rides the description line
            with the abnormal-model detail folded into its tooltip. */}
        <CardDescription className="machine-text flex flex-wrap items-center gap-2">
          <span>{t(`providers.credentials.kinds.${credential.kind}`, { defaultValue: credential.kind })} · #{credential.id}</span>
          <CredentialHealthBadge credentialId={credential.id} health={credential.health} models={credential.model_health} observedAt={credential.health_observed_at} />
        </CardDescription>
        <CardAction className="flex items-center justify-end gap-1">
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
            trigger={
              <Button variant="ghost" size="icon-sm" aria-label={`${t("common.actions.edit")}: ${name}`}>
                <PencilIcon aria-hidden />
              </Button>
            }
          />
          <EntityDeleteButton entity="credentials" id={credential.id} label={name} queryKeys={["credentials", "credential-cycles"]} />
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={t("providers.credentials.quota.title")}
            aria-expanded={quotaOpen}
            onClick={() => setQuotaOpen((open) => !open)}
          >
            <ChevronDownIcon aria-hidden className={quotaOpen ? "transition-transform" : "-rotate-90 transition-transform"} />
          </Button>
        </CardAction>
      </CardHeader>
      {quotaOpen ? (
        <CardContent className="flex flex-col gap-3">
          <div className="flex items-center justify-between gap-2">
            <div className="min-w-0">
              <p className="text-sm font-medium">{t("providers.credentials.quota.title")}</p>
              <p className="text-xs text-muted-foreground">{t("providers.credentials.quota.hint")}</p>
            </div>
            {/* Live upstream call; some upstreams rate-limit it, so it only fires on demand. */}
            <Button variant="outline" size="sm" className="shrink-0" disabled={probe.isFetching || reset.isPending} onClick={() => void refresh()}>
              <RefreshCwIcon aria-hidden className={probe.isFetching ? "animate-spin" : undefined} />
              {probe.isFetching ? t("providers.credentials.quotaProbe.pending") : t("providers.credentials.quotaProbe.action")}
            </Button>
          </div>
          <CredentialCycleList cycles={props.cycles} loading={props.cyclesLoading} error={props.cyclesError} />
            {resetCredits ? (
              <div className="flex items-center justify-between gap-3 rounded-lg border bg-card px-3 py-2">
                <p className="min-w-0 text-sm">
                  <span className="text-muted-foreground">{t("providers.credentials.quotaReset.available")}: </span>
                  <span className="font-medium tabular-nums">{resetCredits.available_count}</span>
                  {resetCredits.expires_at != null ? (
                    <span className="text-xs text-muted-foreground"> · {t("providers.credentials.quotaReset.expires", { time: formatInstant(resetCredits.expires_at, i18n.language) })}</span>
                  ) : null}
                </p>
                <Button
                  variant="outline"
                  size="sm"
                  disabled={resetCredits.available_count === 0 || reset.isPending || probe.isFetching}
                  onClick={() => setResetOpen(true)}
                >
                  <RotateCcwIcon aria-hidden className={reset.isPending ? "animate-spin" : undefined} />
                  {t("providers.credentials.quotaReset.action")}
                </Button>
              </div>
            ) : null}
            {raw ? (
              <Collapsible>
                <CollapsibleTrigger asChild>
                  <Button variant="ghost" size="sm" className="self-start text-muted-foreground">
                    <ChevronsUpDownIcon aria-hidden className="size-3" />
                    {t("providers.credentials.quota.raw")}
                  </Button>
                </CollapsibleTrigger>
                <CollapsibleContent>
                  <pre className="max-h-64 overflow-auto rounded-md bg-muted p-3 text-xs">{raw}</pre>
                </CollapsibleContent>
              </Collapsible>
            ) : null}
        </CardContent>
      ) : null}
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

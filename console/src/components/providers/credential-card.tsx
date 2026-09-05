import type { CredentialDto } from "@/generated/CredentialDto"
import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { ChevronsUpDownIcon, RefreshCwIcon, RotateCcwIcon } from "lucide-react"
import { useMemo, useState } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { ApiError } from "@/api/client"
import { probeCredentialQuota, resetCredentialQuota } from "@/api/control"
import { ConfirmDangerous } from "@/components/confirm-dangerous"
import { CredentialCycleList } from "@/components/providers/credential-cycle-list"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { formatInstant } from "@/lib/format"

type Props = {
  credential: CredentialDto
  cycles: Array<CredentialQuotaCycleDto>
  cyclesLoading: boolean
  cyclesError: boolean
}

export function CredentialCard(props: Props) {
  if (!props.credential.quota_capabilities) return null
  return <SubscriptionCredentialCard {...props} />
}

function SubscriptionCredentialCard(props: Props) {
  const { t, i18n } = useTranslation()
  const credential = props.credential
  const client = useQueryClient()
  const [resetOpen, setResetOpen] = useState(false)
  const probe = useQuery({
    queryKey: ["credential-quota-probe", credential.id, credential.version],
    queryFn: async () => {
      const result = await probeCredentialQuota(credential.id)
      void client.invalidateQueries({ queryKey: ["credential-cycles"] })
      return result
    },
    enabled: credential.quota_capabilities?.probe === true,
    retry: false,
    staleTime: 10 * 60 * 1000,
    gcTime: Infinity,
  })
  const quota = probe.data ?? null
  const manual = useMutation({ mutationFn: () => probeCredentialQuota(credential.id, true) })
  const mergedCycles = useMemo(() => {
    const byId = new Map<number, CredentialQuotaCycleDto>()
    for (const cycle of [...(quota?.cycles ?? []), ...props.cycles]) {
      const current = byId.get(cycle.id)
      if (!current || cycle.version >= current.version) byId.set(cycle.id, cycle)
    }
    return [...byId.values()]
  }, [quota?.cycles, props.cycles])
  const refresh = async () => {
    try {
      const result = await manual.mutateAsync()
      client.setQueryData(["credential-quota-probe", credential.id, credential.version], result)
      await client.invalidateQueries({ queryKey: ["credential-cycles"] })
      toast.success(t("providers.credentials.quotaProbe.success", { count: result.windows.length }))
    } catch (error) {
      toast.error(error instanceof ApiError ? error.message : t("providers.credentials.quotaProbe.error"))
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

  return (
    <>
      <Card size="sm">
        <CardContent className="flex flex-col gap-3">
          <div className="flex items-center justify-between gap-2">
            <div className="min-w-0">
              <p className="text-sm font-medium">{t("providers.credentials.quota.title")}</p>
            </div>
            <Button variant="outline" size="sm" className="shrink-0" disabled={probe.isFetching || manual.isPending || reset.isPending} onClick={() => void refresh()}>
              <RefreshCwIcon aria-hidden className={probe.isFetching ? "animate-spin" : undefined} />
              {probe.isFetching ? t("providers.credentials.quotaProbe.pending") : t("providers.credentials.quotaProbe.action")}
            </Button>
          </div>
          {resetCredits || credential.quota_capabilities?.reset ? (
            <section aria-label={t("providers.credentials.quotaReset.available")} className="flex flex-wrap items-center justify-between gap-3 rounded-lg border bg-card px-3 py-2">
              <p className="min-w-0 text-sm">
                <span className="text-muted-foreground">{t("providers.credentials.quotaReset.available")}: </span>
                <span className="font-medium tabular-nums">{resetCredits?.available_count ?? "—"}</span>
                {resetCredits?.expires_at != null ? (
                  <span className="text-xs text-muted-foreground"> · {t("providers.credentials.quotaReset.expires", { time: formatInstant(resetCredits.expires_at, i18n.language) })}</span>
                ) : null}
              </p>
              {credential.quota_capabilities?.reset ? <Button
                variant="outline"
                size="sm"
                disabled={!resetCredits || resetCredits.available_count <= 0 || reset.isPending || probe.isFetching || manual.isPending}
                onClick={() => setResetOpen(true)}
              >
                <RotateCcwIcon aria-hidden className={reset.isPending ? "animate-spin" : undefined} />
                {t("providers.credentials.quotaReset.action")}
              </Button> : null}
            </section>
          ) : null}
          <CredentialCycleList
            cycles={mergedCycles}
            localError={quota?.local_error}
            windows={quota?.windows}
            loading={!quota && (props.cyclesLoading || probe.isFetching)}
            error={!quota && props.cyclesError}
          />
          {probe.isError ? <p role="alert" className="text-sm text-destructive">{probe.error instanceof ApiError ? probe.error.message : t("providers.credentials.quotaProbe.error")}</p> : null}
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

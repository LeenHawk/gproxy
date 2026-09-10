import type { CredentialDto } from "@/generated/CredentialDto"
import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import { useMutation } from "@tanstack/react-query"
import { ChevronsUpDownIcon, RefreshCwIcon, RotateCcwIcon } from "lucide-react"
import { useMemo, useState } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { ApiError } from "@/api/client"
import { resetCredentialQuota } from "@/api/control"
import { ConfirmDangerous } from "@/components/confirm-dangerous"
import { CredentialCycleList } from "@/components/providers/credential-cycle-list"
import { CredentialQuotaSources } from "@/components/providers/credential-quota-sources"
import { useCredentialQuota } from "@/components/providers/use-credential-quota"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { formatInstant } from "@/lib/format"

type Props = {
  credential: CredentialDto
  cycles: Array<CredentialQuotaCycleDto>
  cyclesLoading: boolean
  cyclesError: boolean
}

export function CredentialCard(props: Props) {
  const { t, i18n } = useTranslation()
  const credential = props.credential
  const [resetOpen, setResetOpen] = useState(false)
  const { snapshot, quota, canProbe, loading, refreshing, error, refresh: probe } = useCredentialQuota(credential)
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
      const result = await probe()
      if (result.snapshot.sources.some((source) => source.error)) toast.error(t("providers.credentials.quotaProbe.error"))
      else toast.success(t("providers.credentials.quotaProbe.success", { count: result.snapshot.entries.length }))
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
  const resetCredits = snapshot?.sources.find((source) => source.reset_credits)?.reset_credits ?? quota?.reset_credits
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
        <CardHeader>
          <CardTitle headingLevel={3}>{t("providers.credentials.quota.title")}</CardTitle>
          {canProbe ? <CardAction><Button variant="outline" size="sm" disabled={refreshing || reset.isPending} onClick={() => void refresh()}>
            <RefreshCwIcon aria-hidden data-icon="inline-start" className={refreshing ? "animate-spin" : undefined} />
            {refreshing ? t("providers.credentials.quotaProbe.pending") : t("providers.credentials.quotaProbe.action")}
          </Button></CardAction> : null}
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          {loading ? <p className="text-sm text-muted-foreground">{t("common.loading")}</p> : null}
          {snapshot ? <CredentialQuotaSources snapshot={snapshot} refreshing={refreshing} /> : null}
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
                disabled={!resetCredits || resetCredits.available_count <= 0 || reset.isPending || refreshing}
                onClick={() => setResetOpen(true)}
              >
                <RotateCcwIcon aria-hidden data-icon="inline-start" className={reset.isPending ? "animate-spin" : undefined} />
                {t("providers.credentials.quotaReset.action")}
              </Button> : null}
            </section>
          ) : null}
          {mergedCycles.length > 0 || snapshot?.entries.some((entry) => entry.value.kind === "window") ? <CredentialCycleList
            cycles={mergedCycles}
            localError={quota?.local_error}
            windows={snapshot?.entries.flatMap((entry) => entry.value.kind === "window" ? [{
              window_key: entry.id, label: entry.label, upstream_used: entry.value.used,
              upstream_limit: entry.value.limit, unit: entry.value.unit,
              used_percent: entry.value.used_percent, period_end: entry.value.period_end,
            }] : []) ?? []}
            loading={loading && props.cyclesLoading}
            error={!snapshot && props.cyclesError}
          /> : null}
          {error ? <p role="alert" className="text-sm text-destructive">{error instanceof ApiError ? error.message : t("providers.credentials.quotaProbe.error")}</p> : null}
            {raw ? (
              <Collapsible>
                <CollapsibleTrigger asChild>
                  <Button variant="ghost" size="sm" className="self-start text-muted-foreground">
                    <ChevronsUpDownIcon aria-hidden data-icon="inline-start" />
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

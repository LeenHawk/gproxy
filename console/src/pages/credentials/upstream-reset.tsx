import { useState } from "react"
import { useMutation, useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import type { QuotaResetOptionDto, QuotaResetWrite } from "@/generated/sdk"
import { quotaResetCredits, resetUpstreamQuota } from "@/api/credentials"
import { formatInstant, formatNumber } from "@/lib/format"
import { ErrorNotice } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { AlertDialog, AlertDialogContent, AlertDialogHeader, AlertDialogTitle, AlertDialogDescription, AlertDialogFooter, AlertDialogCancel } from "@/components/ui/alert-dialog"

export function UpstreamReset({ id, enabled, busy, onReset }: { id: string; enabled: boolean; busy: boolean; onReset: () => Promise<unknown> }) {
  const { t, i18n } = useTranslation()
  const cards = useQuery({ queryKey: ["credential-reset-credits", id], queryFn: () => quotaResetCredits(id), enabled, retry: false })
  const [selection, setSelection] = useState<{ request: QuotaResetWrite; option?: QuotaResetOptionDto } | null>(null)
  const reset = useMutation({ mutationFn: (request: QuotaResetWrite) => resetUpstreamQuota(id, request), retry: false,
    onSuccess: async () => { setSelection(null); await Promise.all([onReset(), cards.refetch()]) } })
  const pending = busy || cards.isFetching || reset.isPending || !enabled
  const choose = (option?: QuotaResetOptionDto) => {
    reset.reset()
    setSelection({ option, request: { program: option?.program ?? null, grantId: option?.grantId ?? null, requestId: crypto.randomUUID() } })
  }
  const text = (value: string) => t(`limits.resetText.${value}`, { defaultValue: value })
  const label = (option: QuotaResetOptionDto) => option.title ? text(option.title) : option.label || t("limits.grantReset")
  const description = (option: QuotaResetOptionDto) => option.description ? text(option.description) : null
  const reason = (value: string) => t(`limits.resetReasons.${value}`, { defaultValue: value })
  const options = cards.data?.options ?? []
  const eligibility = options.length > 0 && cards.data?.availableCount == null
  const available = (cards.data?.availableCount ?? 0) > 0
  const selectedUsable = selection?.option ? options.some(option => option.program === selection.option?.program && option.grantId === selection.option?.grantId && option.usable) : available
  return <>
    <Card className="gap-3 py-4"><CardHeader className="px-4"><CardTitle className="flex flex-wrap items-center justify-between gap-3">
      <span>{eligibility ? t("limits.resetEligibility") : t("limits.resetCredits")}{!eligibility ? <> <span className="tabular-nums">{cards.data?.availableCount == null ? "—" : formatNumber(cards.data.availableCount, i18n.language)}</span></> : null}</span>
      <Button variant="ghost" size="sm" disabled={pending} onClick={() => void cards.refetch()}>{t(eligibility ? "limits.queryResetEligibility" : "limits.queryResetCredits")}</Button>
    </CardTitle></CardHeader><CardContent className="flex flex-col gap-3 px-4">
      {!options.length && cards.data?.creditExpirationsMs?.length ? <dl className="grid grid-cols-3 gap-2">
        {cards.data.creditExpirationsMs.map((expiry, index) => <div key={index} className="min-w-0 rounded-lg border p-2">
          <dt className="text-xs text-muted-foreground">{t("limits.resetCardNumber", { number: index + 1 })}</dt>
          <dd className="mt-1 text-xs tabular-nums">{expiry == null ? "—" : <time dateTime={new Date(expiry).toISOString()} title={t("limits.resetCreditsExpire", { at: formatInstant(expiry, i18n.language) })}>
            <span className="block">{new Intl.DateTimeFormat(i18n.language, { year: "numeric", month: "2-digit", day: "2-digit" }).format(expiry)}</span>
            <span className="block">{new Intl.DateTimeFormat(i18n.language, { hour: "2-digit", minute: "2-digit", hour12: false }).format(expiry)}</span>
          </time>}</dd>
        </div>)}
      </dl> : !options.length && cards.data?.expiresAtMs != null ? <p className="text-xs text-muted-foreground">{t("limits.resetCreditsExpire", { at: formatInstant(cards.data.expiresAtMs, i18n.language) })}</p> : null}
      {options.map(option => <section key={`${option.program}:${option.grantId ?? ""}`} className="flex flex-col gap-2 border-t pt-3" aria-label={label(option)}>
        <div className="flex flex-wrap items-center justify-between gap-2"><p className="text-sm font-medium">{label(option)}</p><Button variant="outline" size="sm" disabled={pending || !option.usable} onClick={() => choose(option)}>{t("management.upstreamReset")}</Button></div>
        {option.label && option.label !== label(option) ? <p className="text-xs text-muted-foreground">{option.label}</p> : null}
        {description(option) ? <p className="text-xs text-muted-foreground">{description(option)}</p> : null}
        <p className="text-sm">{t("limits.resetCount")} {option.availableCount == null ? "—" : formatNumber(option.availableCount, i18n.language)}</p>
        {option.scopeLabels?.length ? <p className="text-xs text-muted-foreground">{t("limits.resetScope", { scope: option.scopeLabels.map(text).join(i18n.language.startsWith("zh") ? "、" : ", ") })}</p> : null}
        {option.expiresAtMs != null ? <p className="text-xs text-muted-foreground">{t("limits.resetCreditsExpire", { at: formatInstant(option.expiresAtMs, i18n.language) })}</p> : null}
        {option.nextAvailableAtMs != null ? <p className="text-xs text-muted-foreground">{t("limits.nextResetAvailable", { at: formatInstant(option.nextAvailableAtMs, i18n.language) })}</p> : null}
        {!option.usable && option.ineligibleReason ? <p className="text-sm text-muted-foreground">{reason(option.ineligibleReason)}</p> : null}
      </section>)}
      {!options.length ? <Button className="self-start" variant="outline" size="sm" disabled={pending || !available} onClick={() => choose()}>{t("management.upstreamReset")}</Button> : null}
      {cards.error ? <ErrorNotice error={cards.error} /> : null}
      {reset.data ? <p className="text-sm" role="status">{t(`limits.resetOutcome.${reset.data.outcome}`)}{reset.data.reason ? ` ${reason(reset.data.reason)}` : ""}</p> : null}
    </CardContent></Card>
    <AlertDialog open={selection != null} onOpenChange={open => { if (!open && !reset.isPending) setSelection(null) }}><AlertDialogContent><AlertDialogHeader>
      <AlertDialogTitle>{t("management.upstreamReset")}</AlertDialogTitle><AlertDialogDescription>{t("management.upstreamResetConfirm")}</AlertDialogDescription>
      {selection?.option && description(selection.option) ? <p className="text-sm text-muted-foreground">{description(selection.option)}</p> : null}
      {selection?.option ? <div className="flex flex-col gap-2 text-sm"><p>{label(selection.option)}</p>{selection.option.scopeLabels?.length ? <p>{t("limits.resetScope", { scope: selection.option.scopeLabels.map(text).join(i18n.language.startsWith("zh") ? "、" : ", ") })}</p> : null}{selection.option.expiresAtMs != null ? <p>{t("limits.resetCreditsExpire", { at: formatInstant(selection.option.expiresAtMs, i18n.language) })}</p> : null}</div> : null}
      {reset.error ? <ErrorNotice error={reset.error} /> : null}
    </AlertDialogHeader>
      <AlertDialogFooter><AlertDialogCancel disabled={reset.isPending}>{t("actions.cancel")}</AlertDialogCancel><Button disabled={pending || !selectedUsable} onClick={() => selection && reset.mutate(selection.request)}>{t("management.upstreamReset")}</Button></AlertDialogFooter>
    </AlertDialogContent></AlertDialog>
  </>
}

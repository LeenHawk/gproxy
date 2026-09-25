import { useState } from "react"
import { useMutation, useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { Check, CircleX, LoaderCircle, Play, RefreshCw } from "lucide-react"
import { discoverWithCredential, testWithCredential } from "@/api/credentials"
import type { CredentialDto } from "@/generated/sdk"
import { ManagementDialog } from "@/components/management-dialog"
import { SearchableSelect } from "@/components/searchable-select"
import { ErrorNotice } from "@/components/state"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Field, FieldLabel } from "@/components/ui/field"

export function CredentialTest({ credential, onClose }: { credential: CredentialDto; onClose: () => void }) {
  const { t } = useTranslation()
  const [model, setModel] = useState("")
  const models = useQuery({
    queryKey: ["credential-test-models", credential.id],
    queryFn: () => discoverWithCredential(credential.id),
    retry: false,
    refetchOnWindowFocus: false,
    staleTime: 60_000,
  })
  const selected = model || models.data?.[0]?.upstreamName || ""
  const test = useMutation({ mutationFn: (name: string) => testWithCredential(credential.id, name) })
  const result = test.data
  return <ManagementDialog title={`${t("providers.models.test")} · ${credential.label ?? credential.id}`} className="top-1/2 bottom-auto -translate-y-1/2 sm:max-w-md" busy={test.isPending} onClose={onClose}>
    <Field>
      <div className="flex items-center justify-between gap-2">
        <FieldLabel htmlFor="credential-test-model">{t("fields.upstreamName")}</FieldLabel>
        <Button variant="ghost" size="icon-sm" disabled={models.isFetching || test.isPending} title={t("management.discover")} aria-label={t("management.discover")} onClick={() => void models.refetch()}>
          {models.isFetching ? <LoaderCircle className="animate-spin" /> : <RefreshCw />}
        </Button>
      </div>
      <fieldset disabled={test.isPending} className="flex min-w-0 items-center gap-2">
        <div className="min-w-0 flex-1"><SearchableSelect id="credential-test-model" label={t("fields.upstreamName")} value={selected} options={(models.data ?? []).map(item => ({ value: item.upstreamName, label: item.upstreamName }))} allowCustom onChange={value => { setModel(value); test.reset() }} /></div>
        <Button disabled={test.isPending || !selected.trim()} onClick={() => test.mutate(selected.trim())}>
          {test.isPending ? <LoaderCircle data-icon="inline-start" className="animate-spin" /> : <Play data-icon="inline-start" />}
          {t("providers.models.test")}
        </Button>
      </fieldset>
    </Field>
    {models.error ? <ErrorNotice error={models.error} /> : null}
    {test.error ? <ErrorNotice error={test.error} /> : null}
    {result ? <section role="status" className="flex min-w-0 flex-col gap-3 border-t pt-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <Badge variant={result.ok ? "success" : "destructive"}>{result.ok ? <Check data-icon="inline-start" /> : <CircleX data-icon="inline-start" />}{t(result.ok ? "modelUI.testOk" : "values.failed")}</Badge>
        <span className="text-xs tabular-nums text-muted-foreground">{result.latencyMs} ms{result.status ? ` · HTTP ${result.status}` : ""}</span>
      </div>
      <p className="break-all text-xs text-muted-foreground">{result.model}</p>
      {result.error ? <p className="whitespace-pre-wrap break-words text-sm text-destructive">{result.error}</p> : null}
      {result.reply ? <div className="max-h-64 overflow-y-auto rounded-lg bg-muted/50 p-3 text-sm leading-relaxed whitespace-pre-wrap break-words">{result.reply}</div> : null}
    </section> : null}
  </ManagementDialog>
}

import { useState } from "react"
import { useMutation, useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { ApiError } from "@/api/client"
import { applyUpdate, checkUpdate, rollbackUpdate, updateSchedule } from "@/api/update"
import { INFO_KEY, instanceInfo, SETTINGS_KEY, readSettings } from "@/api/settings"
import { Page, PageHeader, PageSection } from "@/components/page"
import { ConfirmButton } from "@/components/confirm"
import { EmptyNotice, ErrorNotice, QueryState } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { Field, FieldLabel } from "@/components/ui/field"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Link } from "@/lib/router"
import { formatInstant } from "@/lib/format"

export function UpdatePage() {
  const { t, i18n } = useTranslation()
  const [selected, setSelected] = useState<string | null>(null)
  const info = useQuery({ queryKey: INFO_KEY, queryFn: instanceInfo })
  const settings = useQuery({ queryKey: SETTINGS_KEY, queryFn: readSettings })
  const schedule = useQuery({ queryKey: ["update"], queryFn: updateSchedule, retry: false })
  const channel = selected ?? settings.data?.instance.updateChannel ?? "release"
  const checked = useMutation({ mutationFn: checkUpdate })
  const installed = useMutation({ mutationFn: (kind: "apply" | "rollback") => kind === "apply" ? applyUpdate(channel) : rollbackUpdate() })
  const report = checked.data?.channel === channel ? checked.data : schedule.data?.last_check?.channel === channel ? schedule.data.last_check : null
  const unsupported = schedule.error instanceof ApiError && schedule.error.status === 404
  const busy = checked.isPending || installed.isPending
  const notesUrl = report?.notes_url && /^https?:\/\//.test(report.notes_url) ? report.notes_url : null
  return <Page>
    <PageHeader title={t("nav.update")} actions={<Link to="/settings" className="text-sm underline">{t("nav.settings")}</Link>} />
    <QueryState isPending={info.isPending} error={info.error}><p>{t("update.current")} <Badge variant="outline">{info.data?.version}</Badge> <code title={info.data?.hash}>{info.data?.hash.slice(0, 12)}</code></p></QueryState>
    {unsupported ? <EmptyNotice title={t("update.unsupported")} /> : <QueryState isPending={schedule.isPending} error={schedule.error}>
      {settings.error ? <ErrorNotice error={settings.error} /> : null}
      <Field className="max-w-xs"><FieldLabel htmlFor="update-channel">{t("update.channel")}</FieldLabel><Select value={channel} disabled={busy} onValueChange={(v) => { setSelected(v); checked.reset() }}><SelectTrigger id="update-channel"><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{["dev", "beta", "release"].map((v) => <SelectItem value={v} key={v}>{t(`settingsOption.${v}`)}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
      <p>{schedule.data?.interval_secs ? t("update.schedule", { seconds: schedule.data.interval_secs }) : t("update.noSchedule")} · {t(schedule.data?.automatic ? "update.autoInstall" : "update.manualInstall")}</p>
      <div className="flex flex-wrap items-center gap-2">
        <Button disabled={busy} onClick={() => checked.mutate(channel)}>{checked.isPending ? t("update.checking") : t("update.check")}</Button>
        <ConfirmButton disabled={busy || !report?.available} title={t("update.installConfirm", { version: report?.latest })} confirmLabel={t("update.install")} onConfirm={() => installed.mutate("apply")}>{t("update.install")}</ConfirmButton>
        <ConfirmButton disabled={busy || !report?.rollback_available} title={t("update.rollbackConfirm")} confirmLabel={t("update.rollback")} onConfirm={() => installed.mutate("rollback")}>{t("update.rollback")}</ConfirmButton>
      </div>
      {checked.error || installed.error ? <ErrorNotice error={checked.error ?? installed.error} /> : null}
      {!checked.data && schedule.data?.last_error ? <ErrorNotice error={new Error(schedule.data.last_error)} /> : null}
      {installed.isPending ? <p role="status">{t("update.installing")}</p> : null}
      {installed.data ? <p role="status">{t(installed.data.changed ? "update.installed" : "update.unchanged", { version: installed.data.version ?? "—", restart: installed.data.restart })}</p> : null}
      {report ? <PageSection title={report.available ? t("update.available", { version: report.latest }) : t("update.latest")}>
        <p className="text-sm text-muted-foreground">{report.target} · {formatInstant(report.checked_at_ms, i18n.language)}</p>
        {notesUrl ? <a href={notesUrl} target="_blank" rel="noreferrer" className="text-sm underline">{t("update.notes")}</a> : null}
        {report.notes ? <pre className="whitespace-pre-wrap break-words text-sm">{report.notes}</pre> : null}
      </PageSection> : <p>{t("update.notChecked")}</p>}
    </QueryState>}
  </Page>
}

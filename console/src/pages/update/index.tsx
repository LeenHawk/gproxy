import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { ApiError } from "@/api/client"
import { type UpdateSelection, applyUpdate, checkUpdate, rollbackUpdate, updateProgress, updateSchedule } from "@/api/update"
import { INFO_KEY, instanceInfo, SETTINGS_KEY, readSettings, saveSettings } from "@/api/settings"
import { Page, PageHeader, PageSection } from "@/components/page"
import { DownloadProgress } from "@/components/download-progress"
import { ConfirmButton } from "@/components/confirm"
import { EmptyNotice, ErrorNotice, QueryState } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Link } from "@/components/link"
import { formatInstant } from "@/lib/format"

export function UpdatePage() {
  const { t, i18n } = useTranslation()
  const client = useQueryClient()
  const [selected, setSelected] = useState<string | null>(null)
  const [selectedSource, setSelectedSource] = useState<string | null>(null)
  const info = useQuery({ queryKey: INFO_KEY, queryFn: instanceInfo })
  const settings = useQuery({ queryKey: SETTINGS_KEY, queryFn: readSettings })
  const schedule = useQuery({ queryKey: ["update"], queryFn: updateSchedule, retry: false })
  const savedChannel = settings.data?.instance.updateChannel ?? schedule.data?.channel ?? "release"
  const savedSource = settings.data?.instance.updateSource ?? schedule.data?.source ?? "github"
  const channel = selected ?? savedChannel
  const source = selectedSource ?? savedSource
  const changed = channel !== savedChannel || source !== savedSource
  const checked = useMutation({ mutationFn: checkUpdate })
  const saved = useMutation({
    mutationFn: ({ channel: updateChannel, source: updateSource }: UpdateSelection) => saveSettings({ instance: { updateChannel, updateSource } }),
    onSuccess: (value) => {
      client.setQueryData(SETTINGS_KEY, value)
      setSelected(null)
      setSelectedSource(null)
      void client.invalidateQueries({ queryKey: ["update"] })
      toast.success(t("toast.saved"))
    },
  })
  const installed = useMutation({ mutationFn: (kind: "apply" | "rollback") => kind === "apply" ? applyUpdate({ channel, source }) : rollbackUpdate() })
  const progress = useQuery({
    queryKey: ["update", "progress"],
    queryFn: updateProgress,
    enabled: schedule.isSuccess,
    refetchInterval: 500,
    retry: false,
  })
  const report = checked.data?.channel === channel && checked.data.source === source ? checked.data : schedule.data?.last_check?.channel === channel && schedule.data.last_check.source === source ? schedule.data.last_check : null
  const unsupported = schedule.error instanceof ApiError && schedule.error.status === 404
  const busy = checked.isPending || installed.isPending || saved.isPending || !!progress.data
  const notesUrl = report?.notes_url && /^https?:\/\//.test(report.notes_url) ? report.notes_url : null
  return <Page>
    <PageHeader title={t("nav.update")} actions={<Link to="/settings" className="text-sm underline">{t("nav.settings")}</Link>} />
    <QueryState isPending={info.isPending} error={info.error}><p>{t("update.current")} <Badge variant="outline">{info.data?.version}</Badge> <code title={info.data?.hash}>{info.data?.hash.slice(0, 12)}</code></p></QueryState>
    {unsupported ? <EmptyNotice title={t("update.unsupported")} /> : <QueryState isPending={schedule.isPending} error={schedule.error}>
      {settings.error ? <ErrorNotice error={settings.error} /> : null}
      <FieldGroup className="max-w-xl">
        <Field>
          <FieldLabel htmlFor="update-source">{t("update.source")}</FieldLabel>
          <Select value={source} disabled={busy || !settings.data} onValueChange={(v) => { setSelectedSource(v); checked.reset(); installed.reset(); saved.reset() }}>
            <SelectTrigger id="update-source"><SelectValue /></SelectTrigger>
            <SelectContent><SelectGroup>{["github", "gitlab", "cnb"].map((v) => <SelectItem value={v} key={v}>{t(`settingsOption.${v}`)}</SelectItem>)}</SelectGroup></SelectContent>
          </Select>
        </Field>
        <Field>
          <FieldLabel htmlFor="update-channel">{t("update.channel")}</FieldLabel>
          <Select value={channel} disabled={busy || !settings.data} onValueChange={(v) => { setSelected(v); checked.reset(); installed.reset(); saved.reset() }}>
            <SelectTrigger id="update-channel"><SelectValue /></SelectTrigger>
            <SelectContent><SelectGroup>{["dev", "beta", "release"].map((v) => <SelectItem value={v} key={v}>{t(`settingsOption.${v}`)}</SelectItem>)}</SelectGroup></SelectContent>
          </Select>
        </Field>
      </FieldGroup>
      <FieldDescription>{t("update.channelHelp")}</FieldDescription>
      <div><Button disabled={busy || !settings.data || !changed} onClick={() => saved.mutate({ channel, source })}>{t("actions.save")}</Button></div>
      {saved.error ? <ErrorNotice error={saved.error} /> : null}
      <p>{schedule.data?.interval_secs ? t("update.schedule", { seconds: schedule.data.interval_secs }) : t("update.noSchedule")} · {t(schedule.data?.automatic ? "update.autoInstall" : "update.manualInstall")}</p>
      <div className="flex flex-wrap items-center gap-2">
        <Button disabled={busy} onClick={() => checked.mutate({ channel, source })}>{checked.isPending ? t("update.checking") : t("update.check")}</Button>
        <ConfirmButton disabled={busy || !report?.available} title={t("update.installConfirm", { version: report?.latest })} confirmLabel={t("update.install")} onConfirm={() => installed.mutate("apply")}>{t("update.install")}</ConfirmButton>
        <ConfirmButton disabled={busy || !report?.rollback_available} title={t("update.rollbackConfirm")} confirmLabel={t("update.rollback")} onConfirm={() => installed.mutate("rollback")}>{t("update.rollback")}</ConfirmButton>
      </div>
      {checked.error || installed.error ? <ErrorNotice error={checked.error ?? installed.error} /> : null}
      {!checked.data && channel === schedule.data?.channel && source === schedule.data?.source && schedule.data?.last_error ? <ErrorNotice error={new Error(schedule.data.last_error)} /> : null}
      {progress.data || installed.isPending ? <DownloadProgress
        label={t(progress.data?.phase === "downloading" ? "update.downloading" : progress.data?.phase === "verifying" ? "update.verifying" : "update.installing")}
        downloaded={progress.data?.downloaded_bytes}
        total={progress.data?.total_bytes}
      /> : null}
      {progress.error && installed.isPending ? <ErrorNotice error={progress.error} /> : null}
      {installed.data ? <p role="status">{t(installed.data.changed ? "update.installed" : "update.unchanged", { version: installed.data.version ?? "—", restart: installed.data.restart })}</p> : null}
      {report ? <PageSection title={report.available ? t("update.available", { version: report.latest }) : t("update.latest")}>
        <p className="text-sm text-muted-foreground">{report.target} · {formatInstant(report.checked_at_ms, i18n.language)}</p>
        {notesUrl ? <a href={notesUrl} target="_blank" rel="noreferrer" className="text-sm underline">{t("update.notes")}</a> : null}
        {report.notes ? <pre className="whitespace-pre-wrap break-words text-sm">{report.notes}</pre> : null}
      </PageSection> : <p>{t("update.notChecked")}</p>}
    </QueryState>}
  </Page>
}

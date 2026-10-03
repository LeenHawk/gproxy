//! Updates, and what used to be the About page.
//!
//! One page because they answer one question — "what is this instance
//! running, and should it be running something else?" — in three parts: the
//! build and the project's links, the publisher's notices for this build, and
//! the self-update controls that act on them.

import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { BookOpenIcon, CodeIcon, ExternalLinkIcon, HeartIcon } from "lucide-react"
import { ApiError } from "@/api/client"
import { ANNOUNCEMENTS_KEY, type UpdateSelection, announcements, applyUpdate, checkUpdate, rollbackUpdate, updateProgress, updateSchedule } from "@/api/update"
import { INFO_KEY, instanceInfo, SETTINGS_KEY, readSettings, saveSettings } from "@/api/settings"
import { Page, PageHeader, PageSection } from "@/components/page"
import { AnnouncementList } from "@/components/announcements"
import { DownloadProgress } from "@/components/download-progress"
import { ConfirmButton } from "@/components/confirm"
import { EmptyNotice, ErrorNotice, LoadingRows, QueryState } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Switch } from "@/components/ui/switch"
import { Badge } from "@/components/ui/badge"
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card"
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Link } from "@/components/link"
import { formatInstant } from "@/lib/format"
import { DOCS_URL, REPO_URL, SPONSORS_URL } from "@/lib/project-links"

export function UpdatePage() {
  const { t, i18n } = useTranslation()
  const client = useQueryClient()
  const [selected, setSelected] = useState<string | null>(null)
  const [selectedSource, setSelectedSource] = useState<string | null>(null)
  const info = useQuery({ queryKey: INFO_KEY, queryFn: instanceInfo })
  const settings = useQuery({ queryKey: SETTINGS_KEY, queryFn: readSettings })
  const schedule = useQuery({ queryKey: ["update"], queryFn: updateSchedule, retry: false, refetchInterval: (query) => settings.data?.instance.updateVerifySignature != null && query.state.data?.verify_signature !== settings.data.instance.updateVerifySignature ? 500 : false })
  const unsupported = schedule.error instanceof ApiError && schedule.error.status === 404
  // The host serves the feed from memory for hours; the console has no reason
  // to ask more often than a page open.
  const notices = useQuery({ queryKey: ANNOUNCEMENTS_KEY, queryFn: announcements, enabled: schedule.isSuccess, retry: false, staleTime: 5 * 60 * 1000 })
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
  const signature = useMutation({
    mutationFn: (updateVerifySignature: boolean) => saveSettings({ instance: { updateVerifySignature } }),
    onSuccess: (value) => {
      client.setQueryData(SETTINGS_KEY, value)
      checked.reset()
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
  const signatureSyncing = settings.data?.instance.updateVerifySignature != null && schedule.data?.verify_signature !== settings.data.instance.updateVerifySignature
  const busy = signatureSyncing || checked.isPending || installed.isPending || saved.isPending || signature.isPending || !!progress.data
  const notesUrl = report?.notes_url && /^https?:\/\//.test(report.notes_url) ? report.notes_url : null
  return <Page>
    <PageHeader title={t("nav.update")} actions={<Link to="/settings" className="text-sm underline">{t("nav.settings")}</Link>} />
    <div className="grid max-w-5xl items-start gap-6 xl:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]">
      <Card>
        <CardHeader>
          <div className="mb-3 flex items-center gap-3">
            <img src={`${import.meta.env.BASE_URL}favicon-96x96.png`} width={48} height={48} className="size-12 rounded-lg" alt="" />
            <div className="flex flex-col gap-1">
              <CardTitle>GPROXY</CardTitle>
              <QueryState isPending={info.isPending} error={info.error} rows={1}>
                <p className="text-xs text-muted-foreground">{t("update.current")} <Badge variant="outline">{info.data?.version}</Badge> <code title={info.data?.hash}>{info.data?.hash.slice(0, 12)}</code></p>
              </QueryState>
            </div>
          </div>
          <CardDescription>{t("about.description")}</CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-5">
          <ul className="flex list-disc flex-col gap-3 pl-5 text-sm leading-6">
            <li>{t("about.features.protocols")}</li>
            <li>{t("about.features.accounts")}</li>
            <li>{t("about.features.control")}</li>
          </ul>
          <p className="text-sm leading-6 text-muted-foreground">{t("about.openSource")}</p>
        </CardContent>
        <CardFooter className="flex-wrap gap-2">
          <Button asChild variant="outline">
            <a href={REPO_URL} target="_blank" rel="noopener noreferrer">
              <CodeIcon data-icon="inline-start" aria-hidden />{t("about.source")}
            </a>
          </Button>
          <Button asChild variant="outline">
            <a href={DOCS_URL} target="_blank" rel="noopener noreferrer">
              <BookOpenIcon data-icon="inline-start" aria-hidden />{t("about.documentation")}
            </a>
          </Button>
          <Button asChild variant="ghost">
            <a href={`${REPO_URL}/issues`} target="_blank" rel="noopener noreferrer">
              {t("about.feedback")}<ExternalLinkIcon data-icon="inline-end" aria-hidden />
            </a>
          </Button>
        </CardFooter>
      </Card>
      <Card>
        <CardHeader>
          <HeartIcon className="mb-3 size-6 text-primary" aria-hidden />
          <CardTitle>{t("about.sponsor.title")}</CardTitle>
          <CardDescription>{t("about.sponsor.description")}</CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <p className="text-sm leading-6 text-muted-foreground">{t("about.sponsor.thanks")}</p>
          <Button asChild className="w-full">
            <a href={SPONSORS_URL} target="_blank" rel="noopener noreferrer">
              <HeartIcon data-icon="inline-start" aria-hidden />{t("about.sponsor.action")}
              <ExternalLinkIcon data-icon="inline-end" aria-hidden />
            </a>
          </Button>
        </CardContent>
      </Card>
    </div>
    {unsupported ? <EmptyNotice title={t("update.unsupported")} /> : <QueryState isPending={schedule.isPending} error={schedule.error}>
      <PageSection title={t("update.announcements.title")}>
        <p className="text-sm text-muted-foreground">{t("update.announcements.help")}</p>
        {notices.isPending ? <LoadingRows rows={1} /> : notices.error ? <ErrorNotice error={notices.error} /> : <AnnouncementList notices={notices.data ?? []} />}
      </PageSection>
      <PageSection title={t("update.selfUpdate")}>
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
        <Field className="max-w-xl">
          <Field orientation="horizontal">
            <Switch id="update-verify-signature" checked={settings.data?.instance.updateVerifySignature ?? schedule.data?.verify_signature ?? true} disabled={busy || !settings.data} onCheckedChange={(value) => signature.mutate(value)} aria-describedby="update-signature-help" />
            <FieldLabel htmlFor="update-verify-signature">{t("update.verifySignature")}</FieldLabel>
          </Field>
          <FieldDescription id="update-signature-help">{t("update.verifySignatureHelp")}</FieldDescription>
        </Field>
        {signature.error ? <ErrorNotice error={signature.error} /> : null}
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
      </PageSection>
    </QueryState>}
  </Page>
}

import { useRef, useState, type FormEvent, type ReactNode } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { ArrowLeft, ArrowRight, Check, Copy, FolderOpen, LoaderCircle } from "lucide-react"
import { toast } from "sonner"
import { completeSetup, setupStatus, pickDataDirectory, androidSetup, validListeningAddress, type SetupStatus, type SetupResult } from "@/api/setup"
import { normalizeSourceMasterKey, parseConfiguration } from "@/api/transfer"
import type { ConfigurationExportDto } from "@/generated/sdk"
import { copyText } from "@/lib/copy-text"
import { SUPPORTED_LANGS, setLanguage, type LangCode } from "@/i18n"
import { ErrorNotice, LoadingRows } from "@/components/state"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardHeader, CardTitle, CardDescription, CardContent, CardFooter } from "@/components/ui/card"
import { Field, FieldGroup, FieldLabel, FieldDescription, FieldError } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"

const SETUP_KEY = ["desktop", "setup"] as const

export default function SetupGate({ children }: { children: ReactNode }) {
  const { t } = useTranslation()
  const status = useQuery({ queryKey: SETUP_KEY, queryFn: setupStatus, retry: false, refetchOnWindowFocus: false })
  if (status.isPending) return <main className="mx-auto max-w-2xl px-5 py-16"><LoadingRows /></main>
  if (status.error) return <main className="mx-auto flex max-w-2xl flex-col gap-4 px-5 py-16"><ErrorNotice error={status.error} /><Button onClick={() => void status.refetch()}>{t("setup.retry")}</Button></main>
  return status.data.required ? <SetupWizard initial={status.data} /> : children
}

export function SetupWizard({ initial }: { initial: SetupStatus }) {
  const { t, i18n } = useTranslation()
  const client = useQueryClient()
  const [step, setStep] = useState(0)
  const [host, setHost] = useState(initial.host)
  const [port, setPort] = useState(String(initial.port))
  const [dataDir, setDataDir] = useState(initial.dataDir)
  const [adminUser, setAdminUser] = useState(initial.adminUser)
  const [password, setPassword] = useState("")
  const [confirmation, setConfirmation] = useState("")
  const [apiKey, setApiKey] = useState("")
  const [autoStart, setAutoStart] = useState(initial.canAutoStart && initial.autoStart)
  const [tray, setTray] = useState(initial.tray)
  const [started, setStarted] = useState(initial.started)
  const [databaseKind, setDatabaseKind] = useState<"sqlite" | "postgres" | "mysql">(initial.database.kind === "url" ? (initial.database.dsn.startsWith("mysql:") ? "mysql" : "postgres") : "sqlite")
  const [databasePath, setDatabasePath] = useState(initial.database.kind === "sqlite" ? initial.database.path : "gproxy.db")
  const [databaseUrl, setDatabaseUrl] = useState(initial.database.kind === "url" ? initial.database.dsn : "")
  const [document, setDocument] = useState<ConfigurationExportDto | null>(null)
  const [sourceKey, setSourceKey] = useState("")
  const [errors, setErrors] = useState<Record<string, string>>({})
  const [fileError, setFileError] = useState<unknown>(null)
  const [importName, setImportName] = useState("")
  const [readingFile, setReadingFile] = useState(false)
  const fileRead = useRef(0)
  const [result, setResult] = useState<SetupResult | null>(null)
  const android = androidSetup()
  const permissions = useQuery({ queryKey: ["desktop", "permissions"],
    queryFn: () => JSON.parse(android!.permissionStatus()) as { notifications: boolean; background: boolean },
    enabled: !!android && step === 2, refetchInterval: step === 2 ? 2000 : false })
  const directory = useMutation({ mutationFn: pickDataDirectory, onSuccess: selected => { if (selected) setDataDir(selected) } })
  const setup = useMutation({ mutationFn: async () => {
    let sourceMasterKey: string | null
    try { sourceMasterKey = normalizeSourceMasterKey(sourceKey) }
    catch { throw new Error(t("management.sourceKeyInvalid")) }
    return completeSetup({ dataDir, host: host.trim(), port: Number(port), adminUser: adminUser.trim(), password,
      apiKey: apiKey || null, autoStart: initial.canAutoStart && autoStart, tray: initial.canChooseDataDir && tray,
      database: databaseKind === "sqlite" ? { kind: "sqlite", path: databasePath.trim() } : { kind: "url", dsn: databaseUrl.trim() },
      import: document ? { export: document, mode: "merge", sourceMasterKey } : null })
  }, onError: async () => {
    try { setStarted((await setupStatus()).started) } catch { /* Keep the original setup error visible. */ }
  }, onSuccess: value => {
    setResult(value); setPassword(""); setConfirmation(""); setSourceKey(""); setApiKey(""); setDocument(null)
    android?.setupFinished()
  } })
  const busy = setup.isPending || directory.isPending || readingFile
  const titles = [t("setup.connection"), t("setup.account"), t("setup.importTitle")]
  const descriptions = [t("setup.connectionHelp"), t("setup.accountHelp"), t("setup.importHelp")]
  function next(event: FormEvent) {
    event.preventDefault()
    const invalid: Record<string, string> = {}
    if (step === 0) {
      if (!validListeningAddress(host)) invalid.host = t("setup.hostRequired")
      if (!/^\d+$/.test(port) || Number(port) < 1 || Number(port) > 65535) invalid.port = t("setup.portInvalid")
      if (!dataDir.trim()) invalid.dataDir = t("setup.directoryRequired")
      if (databaseKind === "sqlite" && !databasePath.trim()) invalid.database = t("setup.databaseRequired")
      if (databaseKind !== "sqlite" && !(databaseKind === "mysql" ? /^mysql:\/\// : /^postgres(?:ql)?:\/\//).test(databaseUrl.trim())) invalid.database = t("setup.databaseInvalid")
    }
    if (step === 1) {
      if (!adminUser.trim()) invalid.adminUser = t("setup.userRequired")
      if (Array.from(password).length < 8 || !password.trim()) invalid.password = t("setup.passwordInvalid")
      if (password !== confirmation) invalid.confirmation = t("setup.passwordMismatch")
    }
    setErrors(invalid)
    if (Object.keys(invalid).length) return
    if (step < 2) { setup.reset(); setStep(step + 1) }
    else setup.mutate()
  }
  async function copy(value: string) {
    try { await copyText(value); toast.success(t("actions.copied")) }
    catch { toast.error(t("setup.copyFailed")) }
  }
  return <main className="mx-auto flex min-h-svh w-full max-w-2xl flex-col justify-center gap-6 px-4 py-8 sm:px-6 sm:py-12">
    <header className="flex items-center justify-between gap-4">
      <div className="flex min-w-0 items-center gap-2"><img src={`${import.meta.env.BASE_URL}favicon-96x96.png`} alt="" className="size-10" /><span className="font-semibold">GPROXY</span></div>
      <Select value={i18n.language} onValueChange={value => void setLanguage(value as LangCode)}><SelectTrigger aria-label={t("setup.language")} className="w-auto"><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{SUPPORTED_LANGS.map(code => <SelectItem key={code} value={code}>{t(`language.${code}`)}</SelectItem>)}</SelectGroup></SelectContent></Select>
    </header>
    {!result ? <ol className="flex justify-between gap-2" aria-label={t("setup.steps")}>{titles.map((title, index) => <li key={title} className="flex min-w-0 items-center gap-2 text-sm" aria-current={step === index ? "step" : undefined}><Badge variant={step === index ? "default" : "secondary"}>{index + 1}</Badge><span className="truncate">{title}</span></li>)}</ol> : null}
    {result ? <Card>
      <CardHeader><CardTitle headingLevel={1}>{t("setup.ready")}</CardTitle><CardDescription>{t("setup.readyHelp")}</CardDescription></CardHeader>
      <CardContent className="flex min-w-0 flex-col gap-5"><FieldGroup>
        <Field data-field-span="full"><FieldLabel htmlFor="setup-url">{t("setup.baseUrl")}</FieldLabel><div className="flex gap-2"><Input id="setup-url" value={result.baseUrl} readOnly /><Button variant="outline" size="icon" aria-label={t("setup.copyUrl")} onClick={() => void copy(result.baseUrl)}><Copy /></Button></div></Field>
        <Field data-field-span="full"><FieldLabel htmlFor="setup-key">{t("setup.gatewayKey")}</FieldLabel><div className="flex gap-2"><Input id="setup-key" type="password" value={result.apiKey} readOnly autoComplete="off" /><Button variant="outline" size="icon" aria-label={t("setup.copyKey")} onClick={() => void copy(result.apiKey)}><Copy /></Button></div><FieldDescription>{t("setup.keyHelp")}</FieldDescription></Field>
      </FieldGroup>
      {result.existingAdminPreserved ? <Alert><AlertTitle>{t("setup.existingAdmin")}</AlertTitle><AlertDescription>{t("setup.existingAdminHelp")}</AlertDescription></Alert> : null}
      {host === "0.0.0.0" || host === "::" ? <p className="text-sm text-muted-foreground">{t("setup.lanUrlHelp")}</p> : null}
      {result.importReport ? <Alert><Check /><AlertTitle>{t("management.importResult")}</AlertTitle><AlertDescription><p>{t("management.importCounts", result.importReport)}</p>{result.importReport.warnings.map((warning, index) => <p key={index}>{warning}</p>)}</AlertDescription></Alert> : null}
      </CardContent><CardFooter><Button className="w-full" onClick={() => client.setQueryData(SETUP_KEY, { ...initial, required: false, completed: true })}>{t("setup.openConsole")}<ArrowRight data-icon="inline-end" /></Button></CardFooter>
    </Card> : <form onSubmit={next} noValidate><Card>
      <CardHeader><CardTitle headingLevel={1}>{titles[step]}</CardTitle><CardDescription>{descriptions[step]}</CardDescription></CardHeader>
      <CardContent className="flex flex-col gap-5">
      {step === 0 ? <FieldGroup>
        <Field data-invalid={!!errors.host}><FieldLabel htmlFor="setup-host">{t("setup.host")}</FieldLabel><Input id="setup-host" value={host} onChange={e => setHost(e.target.value)} disabled={busy || started} aria-invalid={!!errors.host} /><FieldDescription>{t("setup.hostHelp")}</FieldDescription><FieldError>{errors.host}</FieldError></Field>
        <Field data-invalid={!!errors.port}><FieldLabel htmlFor="setup-port">{t("setup.port")}</FieldLabel><Input id="setup-port" inputMode="numeric" value={port} onChange={e => setPort(e.target.value)} disabled={busy || started} aria-invalid={!!errors.port} /><FieldDescription>{t("setup.portHelp")}</FieldDescription><FieldError>{errors.port}</FieldError></Field>
        <Field data-field-span="full" data-invalid={!!errors.dataDir}><FieldLabel htmlFor="setup-directory">{t("setup.dataDir")}</FieldLabel><div className="flex gap-2"><Input id="setup-directory" value={dataDir} onChange={e => setDataDir(e.target.value)} readOnly={!initial.canChooseDataDir} disabled={busy || started} aria-invalid={!!errors.dataDir} />{initial.canChooseDataDir ? <Button type="button" variant="outline" size="icon" aria-label={t("setup.chooseDirectory")} disabled={busy || started} onClick={() => directory.mutate()}><FolderOpen /></Button> : null}</div><FieldDescription>{t(initial.canChooseDataDir ? "setup.directoryHelp" : "setup.privateDirectory")}</FieldDescription><FieldError>{errors.dataDir}</FieldError></Field>
        <Field><FieldLabel htmlFor="setup-database-kind">{t("setup.database")}</FieldLabel><Select value={databaseKind} onValueChange={value => { setDatabaseKind(value as typeof databaseKind); setDatabaseUrl("") }} disabled={busy || started}><SelectTrigger id="setup-database-kind"><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{initial.databaseKinds.map(kind => <SelectItem key={kind} value={kind}>{{ sqlite: "SQLite", postgres: "PostgreSQL", mysql: "MySQL" }[kind]}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
        <Field data-invalid={!!errors.database}><FieldLabel htmlFor="setup-database-value">{t(databaseKind === "sqlite" ? "setup.databaseFile" : "setup.databaseUrl")}</FieldLabel><Input id="setup-database-value" type={databaseKind === "sqlite" ? "text" : "password"} autoComplete="off" value={databaseKind === "sqlite" ? databasePath : databaseUrl} onChange={e => databaseKind === "sqlite" ? setDatabasePath(e.target.value) : setDatabaseUrl(e.target.value)} disabled={busy || started} aria-invalid={!!errors.database} placeholder={databaseKind === "sqlite" ? "gproxy.db" : `${databaseKind}://user:password@host:${databaseKind === "mysql" ? "3306" : "5432"}/gproxy`} /><FieldDescription>{t(databaseKind === "sqlite" ? "setup.databaseFileHelp" : "setup.databaseUrlHelp")}</FieldDescription><FieldError>{errors.database}</FieldError></Field>
        {initial.canAutoStart ? <Field data-field-span="full" orientation="horizontal"><FieldLabel htmlFor="setup-autostart">{t(initial.canChooseDataDir ? "setup.autoStart" : "setup.androidAutoStart")}</FieldLabel><Switch id="setup-autostart" checked={autoStart} onCheckedChange={setAutoStart} disabled={busy} /></Field> : null}
        {initial.canChooseDataDir ? <Field data-field-span="full" orientation="horizontal"><FieldLabel htmlFor="setup-tray">{t("setup.tray")}</FieldLabel><Switch id="setup-tray" checked={tray} onCheckedChange={setTray} disabled={busy} /></Field> : null}
      </FieldGroup> : null}
      {step === 1 ? <FieldGroup>
        <Field data-field-span="full" data-invalid={!!errors.adminUser}><FieldLabel htmlFor="setup-user">{t("setup.adminUser")}</FieldLabel><Input id="setup-user" value={adminUser} onChange={e => setAdminUser(e.target.value)} autoComplete="username" disabled={busy || started} aria-invalid={!!errors.adminUser} /><FieldError>{errors.adminUser}</FieldError></Field>
        <Field data-invalid={!!errors.password}><FieldLabel htmlFor="setup-password">{t("setup.password")}</FieldLabel><Input id="setup-password" type="password" value={password} onChange={e => setPassword(e.target.value)} autoComplete="new-password" disabled={busy} aria-invalid={!!errors.password} /><FieldDescription>{t("setup.passwordHelp")}</FieldDescription><FieldError>{errors.password}</FieldError></Field>
        <Field data-invalid={!!errors.confirmation}><FieldLabel htmlFor="setup-confirmation">{t("setup.confirmation")}</FieldLabel><Input id="setup-confirmation" type="password" value={confirmation} onChange={e => setConfirmation(e.target.value)} autoComplete="new-password" disabled={busy} aria-invalid={!!errors.confirmation} /><FieldError>{errors.confirmation}</FieldError></Field>
        <Field data-field-span="full"><FieldLabel htmlFor="setup-api-key">{t("setup.initialKey")}</FieldLabel><Input id="setup-api-key" type="password" value={apiKey} onChange={e => setApiKey(e.target.value)} autoComplete="off" disabled={busy} placeholder={t("setup.generateKey")} /><FieldDescription>{t("setup.initialKeyHelp")}</FieldDescription></Field>
      </FieldGroup> : null}
      {step === 2 ? <FieldGroup>
        <Field data-field-span="full"><FieldLabel htmlFor="setup-import">{t("management.configurationFile")}</FieldLabel><Input id="setup-import" type="file" accept=".json,application/json" disabled={busy} onChange={async e => {
          const file = e.target.files?.[0]; const reading = ++fileRead.current
          setDocument(null); setImportName(""); setFileError(null); setup.reset(); setSourceKey("")
          if (!file) { setReadingFile(false); return }
          setReadingFile(true)
          try {
            const parsed = parseConfiguration(await file.text())
            if (reading === fileRead.current) { setDocument(parsed); setImportName(file.name) }
          } catch { if (reading === fileRead.current) setFileError(new Error(t("setup.invalidImport"))) }
          finally { if (reading === fileRead.current) setReadingFile(false) }
        }} /><FieldDescription>{readingFile ? t("setup.readingFile") : document ? t("setup.selectedFile", { name: importName }) : t("setup.skipImport")}</FieldDescription></Field>
        {document ? <><Field data-field-span="full"><FieldDescription>{t("management.fileSummary", { version: document.formatVersion, secrets: document.secretsOmitted ? t("management.omitted") : t("management.included") })}</FieldDescription></Field><Field data-field-span="full"><FieldLabel htmlFor="setup-source-key">{t("management.sourceKey")}</FieldLabel><Input id="setup-source-key" type="password" autoComplete="off" value={sourceKey} onChange={e => setSourceKey(e.target.value)} disabled={busy} /><FieldDescription>{t("management.sourceKeyHelp")}</FieldDescription></Field></> : null}
        {android ? <Field data-field-span="full"><FieldLabel>{t("setup.permissions")}</FieldLabel><FieldDescription>{t("setup.permissionsHelp")}</FieldDescription><div className="flex flex-wrap gap-2"><Button type="button" variant="outline" disabled={busy || permissions.data?.notifications} onClick={() => android.requestNotifications()}>{t(permissions.data?.notifications ? "setup.notificationsGranted" : "setup.allowNotifications")}</Button><Button type="button" variant="outline" disabled={busy || permissions.data?.background} onClick={() => android.requestBackground()}>{t(permissions.data?.background ? "setup.backgroundGranted" : "setup.allowBackground")}</Button></div></Field> : null}
      </FieldGroup> : null}
      {fileError || directory.error || setup.error ? <ErrorNotice error={fileError ?? directory.error ?? setup.error} /> : null}
      </CardContent>
      <CardFooter className="justify-between gap-3"><Button type="button" variant="ghost" disabled={busy || step === 0} onClick={() => { setStep(step - 1); setErrors({}); setup.reset() }}><ArrowLeft data-icon="inline-start" />{t("setup.back")}</Button><Button type="submit" disabled={busy || (step === 2 && !!fileError)}>{setup.isPending ? <LoaderCircle data-icon="inline-start" className="animate-spin" /> : null}{t(step === 2 ? "setup.finish" : "setup.next")}{step < 2 ? <ArrowRight data-icon="inline-end" /> : null}</Button></CardFooter>
    </Card></form>}
    <p className="text-center text-xs text-muted-foreground">{t("setup.footer")}</p>
  </main>
}

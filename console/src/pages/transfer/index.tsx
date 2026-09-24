import { useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { parseConfiguration } from "@/api/transfer"
import { api, json } from "@/api/client"
import type { ConfigurationExportDto, ImportReportDto } from "@/generated/sdk"
import { Page, PageHeader, PageSection } from "@/components/page"
import { ConfirmButton } from "@/components/confirm"
import { ErrorNotice } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"

export function TransferPage() {
  const { t } = useTranslation()
  const client = useQueryClient()
  const [includeSecrets, setIncludeSecrets] = useState(false)
  const [document, setDocument] = useState<ConfigurationExportDto | null>(null)
  const [mode, setMode] = useState<"merge" | "replace">("merge")
  const [masterKey, setMasterKey] = useState("")
  const [parseError, setParseError] = useState<unknown>(null)
  const exported = useMutation({ mutationFn: async () => {
    const data = await api<ConfigurationExportDto>("/admin/api/export", json("POST", { includeSecrets }))
    const url = URL.createObjectURL(new Blob([JSON.stringify(data, null, 2)], { type: "application/json" }))
    const link = window.document.createElement("a"); link.href = url; link.download = `gproxy-configuration-${Date.now()}.json`; link.click()
    setTimeout(() => URL.revokeObjectURL(url), 1000)
  } })
  const imported = useMutation({ mutationFn: async () => {
    try { return await api<ImportReportDto>("/admin/api/import", json("POST", { export: document, mode, sourceMasterKey: masterKey.trim() || null })) }
    finally { setMasterKey("") }
  }, onSuccess: async () => { setDocument(null); await client.invalidateQueries(); window.dispatchEvent(new Event("gproxy:context-refresh")) } })
  const busy = imported.isPending || exported.isPending
  return <Page><PageHeader title={t("nav.transfer")} /><p className="text-sm text-muted-foreground">{t("management.transferHelp")}</p>
    <PageSection title={t("management.export")}><Field orientation="horizontal"><FieldLabel htmlFor="transfer-secrets">{t("management.includeSecrets")}</FieldLabel><Switch id="transfer-secrets" checked={includeSecrets} onCheckedChange={setIncludeSecrets} disabled={busy} /></Field><Button disabled={busy} onClick={() => exported.mutate()}>{t("management.export")}</Button></PageSection>
    <PageSection title={t("management.import")}><FieldGroup>
      <Field><FieldLabel htmlFor="transfer-file">{t("management.configurationFile")}</FieldLabel><Input id="transfer-file" type="file" accept=".json,application/json" disabled={busy} onChange={async event => {
        const file = event.target.files?.[0]; setDocument(null); setParseError(null); imported.reset(); setMasterKey("")
        if (!file) return
        try { setDocument(parseConfiguration(await file.text())) } catch (error) { setParseError(error) }
      }} /></Field>
      {document ? <><p>{t("management.fileSummary", { version: document.formatVersion, secrets: document.secretsOmitted ? t("management.omitted") : t("management.included") })}</p><p className="text-sm text-muted-foreground">{t("management.summaryOnly")}</p><dl className="grid grid-cols-2 gap-2 text-sm">{Object.entries(document.data).map(([name, rows]) => <div key={name}><dt>{t(`management.tables.${name}`, { defaultValue: name })}</dt><dd>{Array.isArray(rows) ? rows.length : rows ? 1 : 0}</dd></div>)}</dl></> : null}
      <Field><FieldLabel htmlFor="transfer-mode">{t("management.importMode")}</FieldLabel><Select value={mode} onValueChange={value => setMode(value as typeof mode)} disabled={busy}><SelectTrigger id="transfer-mode"><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{["merge", "replace"].map(value => <SelectItem key={value} value={value}>{t(`management.${value}`)}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
      <Field><FieldLabel htmlFor="transfer-key">{t("management.sourceKey")}</FieldLabel><Input id="transfer-key" type="password" autoComplete="off" value={masterKey} disabled={busy} onChange={event => setMasterKey(event.target.value)} /><p className="text-sm text-muted-foreground">{t("management.sourceKeyHelp")}</p></Field>
      <ConfirmButton disabled={busy || !document} title={t(mode === "replace" ? "management.replaceConfirm" : "management.mergeConfirm")} confirmLabel={t("management.import")} onConfirm={() => imported.mutate()}>{t("management.import")}</ConfirmButton>
    </FieldGroup></PageSection>
    {parseError || exported.error || imported.error ? <ErrorNotice error={parseError ?? exported.error ?? imported.error} /> : null}
    {imported.data ? <PageSection title={t("management.importResult")}><p>{t("management.importCounts", imported.data)}</p>{imported.data.warnings.map((warning, index) => <p key={index} className="break-words text-sm">{warning}</p>)}</PageSection> : null}
  </Page>
}

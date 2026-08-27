import { useMutation, useQueryClient } from "@tanstack/react-query"
import { DownloadIcon, UploadIcon } from "lucide-react"
import { useState } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { exportConfiguration, importConfiguration } from "@/api/control"
import type { ConfigurationExportDto } from "@/generated/ConfigurationExportDto"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"

export function ConfigurationTransferCard() {
  const { t } = useTranslation()
  const client = useQueryClient()
  const [includeSecrets, setIncludeSecrets] = useState(false)
  const [file, setFile] = useState<ConfigurationExportDto | null>(null)
  const [fileName, setFileName] = useState("")
  const [sourceKey, setSourceKey] = useState("")
  const exporting = useMutation({
    mutationFn: exportConfiguration,
    onSuccess: (value) => {
      const url = URL.createObjectURL(new Blob([JSON.stringify(value, null, 2)], { type: "application/json" }))
      const link = document.createElement("a")
      link.href = url
      link.download = `gproxy-config-${new Date().toISOString().replaceAll(":", "-")}.json`
      link.click()
      URL.revokeObjectURL(url)
      toast.success(t("settings.transfer.exported"))
    },
    onError: () => toast.error(t("settings.transfer.exportError")),
  })
  const importing = useMutation({
    mutationFn: importConfiguration,
    onSuccess: async (value) => {
      await client.invalidateQueries()
      toast.success(t("settings.transfer.imported", { count: value.imported }))
    },
    onError: () => toast.error(t("settings.transfer.importError")),
  })
  const choose = async (selected?: File) => {
    if (!selected) return
    try {
      setFile(JSON.parse(await selected.text()) as ConfigurationExportDto)
      setFileName(selected.name)
      setSourceKey("")
    } catch {
      setFile(null)
      setFileName("")
      toast.error(t("settings.transfer.invalidFile"))
    }
  }
  const needsSourceKey = file?.secrets === "included" && file.source_key?.mode === "sealed"
  return (
    <Card>
      <CardHeader><CardTitle>{t("settings.transfer.title")}</CardTitle><CardDescription>{t("settings.transfer.description")}</CardDescription></CardHeader>
      <CardContent className="grid gap-6 lg:grid-cols-2">
        <div className="flex flex-col gap-4">
          <Field orientation="horizontal"><div><FieldLabel htmlFor="export-secrets">{t("settings.transfer.includeSecrets")}</FieldLabel><FieldDescription>{t("settings.transfer.includeSecretsHint")}</FieldDescription></div><Switch id="export-secrets" checked={includeSecrets} onCheckedChange={setIncludeSecrets} /></Field>
          {includeSecrets ? <Alert variant="destructive"><AlertTitle>{t("settings.transfer.secretWarningTitle")}</AlertTitle><AlertDescription>{t("settings.transfer.secretWarning")}</AlertDescription></Alert> : null}
          <Button type="button" variant="outline" disabled={exporting.isPending} onClick={() => exporting.mutate({ include_secrets: includeSecrets })}><DownloadIcon data-icon="inline-start" />{t(exporting.isPending ? "settings.transfer.exporting" : "settings.transfer.export")}</Button>
        </div>
        <div className="flex flex-col gap-4">
          <Field><FieldLabel htmlFor="configuration-file">{t("settings.transfer.file")}</FieldLabel><Input id="configuration-file" type="file" accept="application/json,.json" onChange={(event) => void choose(event.target.files?.[0])} /><FieldDescription>{fileName || t("settings.transfer.fileHint")}</FieldDescription></Field>
          {needsSourceKey ? <Field><FieldLabel htmlFor="source-master-key">{t("settings.transfer.sourceKey")}</FieldLabel><Input id="source-master-key" type="password" autoComplete="off" value={sourceKey} onChange={(event) => setSourceKey(event.target.value)} /><FieldDescription>{t("settings.transfer.sourceKeyHint")}</FieldDescription></Field> : null}
          {file?.secrets === "included" ? <Alert variant="destructive"><AlertTitle>{t("settings.transfer.secretFileTitle")}</AlertTitle><AlertDescription>{t("settings.transfer.secretFile")}</AlertDescription></Alert> : null}
          <Button type="button" disabled={!file || importing.isPending || (needsSourceKey && !sourceKey)} onClick={() => file && importing.mutate({ export: file, source_master_key: sourceKey || null })}><UploadIcon data-icon="inline-start" />{t(importing.isPending ? "settings.transfer.importing" : "settings.transfer.import")}</Button>
        </div>
      </CardContent>
    </Card>
  )
}

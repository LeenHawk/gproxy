import { useId, useMemo, useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { credentials } from "@/api/configuration"
import { invalidateConfiguration } from "@/api/invalidation"
import type { CredentialProviderDto } from "@/generated/app"
import { ManagementDialog } from "@/components/management-dialog"
import { ErrorNotice } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Textarea } from "@/components/ui/textarea"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { authKinds } from "@/pages/providers/fields"
import { ownerColumns, useOwnerChoices } from "./ownership"
import { parseBulkCredentials, type BulkFormat } from "./bulk-parse"

export function CredentialBulkImport({ provider, onClose }: { provider: CredentialProviderDto; onClose: () => void }) {
  const { t } = useTranslation()
  const id = useId()
  const client = useQueryClient()
  const ownership = useOwnerChoices()
  const kinds = provider.channel === "devin" ? authKinds.filter(kind => kind.value === "oauth") : authKinds
  const [owner, setOwner] = useState(ownership.defaultOwner)
  const [authKind, setAuthKind] = useState(provider.loginModes.includes("api_key") ? "api_key" : provider.loginModes.includes("cookie") ? "cookie" : "oauth")
  const [format, setFormat] = useState<BulkFormat>("json")
  const [input, setInput] = useState("")
  const parsed = useMemo(() => parseBulkCredentials(input, format), [input, format])
  const label = (item: typeof parsed.items[number]) => item.label ?? t("credentialImport.defaultLabel", { index: item.source })
  const save = useMutation({ mutationFn: () => credentials.batch(parsed.items.map(item => ({ create: {
    id: null, providerId: provider.id, label: label(item), authKind, secret: item.secret,
    enabled: true, metadata: null, connectionProfileId: null, proxy: null, expiresAtMs: null,
    ...ownerColumns(owner),
  } }))), onSuccess: async rows => {
    setInput("")
    await invalidateConfiguration(client, credentials.path)
    toast.success(t("credentialImport.imported", { count: rows.length }))
    onClose()
  } })
  const blocked = !provider.enabled || save.isPending || !!ownership.error || !parsed.items.length || !!parsed.errors.length
  return <ManagementDialog title={t("credentialImport.title")} className="sm:max-w-2xl" onClose={onClose} busy={save.isPending}>
    <form className="flex min-w-0 flex-col gap-4" onSubmit={event => { event.preventDefault(); if (!blocked) save.mutate() }}>
      <fieldset disabled={save.isPending} className="min-w-0">
        <FieldGroup>
          <Field><FieldLabel htmlFor={`${id}-auth`}>{t("fields.authKind")}</FieldLabel><Select value={authKind} onValueChange={value => { setAuthKind(value); setFormat("json"); save.reset() }}><SelectTrigger id={`${id}-auth`}><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{kinds.map(kind => <SelectItem key={kind.value} value={kind.value}>{kind.label}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
          <Field><FieldLabel htmlFor={`${id}-owner`}>{t("management.owner")}</FieldLabel><Select value={owner} onValueChange={setOwner}><SelectTrigger id={`${id}-owner`}><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{ownership.choices.map(choice => <SelectItem key={choice.value} value={choice.value}>{choice.label}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
          <Field><FieldLabel htmlFor={`${id}-format`}>{t("credentialImport.format")}</FieldLabel><Select value={format} onValueChange={value => { setFormat(value as BulkFormat); save.reset() }}><SelectTrigger id={`${id}-format`}><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value="json">JSON / JSONL</SelectItem>{authKind === "api_key" ? <SelectItem value="tokens">{t("credentialImport.tokens")}</SelectItem> : null}</SelectGroup></SelectContent></Select></Field>
          <Field data-field-span="full" data-invalid={parsed.errors.length > 0}>
            <FieldLabel htmlFor={`${id}-input`}>{t("credentialImport.input")}</FieldLabel>
            <Textarea id={`${id}-input`} rows={8} value={input} spellCheck={false} autoComplete="off" className="font-mono" aria-invalid={parsed.errors.length > 0} aria-describedby={`${id}-hint${parsed.errors.length ? ` ${id}-errors` : ""}`} onChange={event => { setInput(event.target.value); save.reset() }} />
            <FieldDescription id={`${id}-hint`}>{t(format === "json" ? "credentialImport.jsonHint" : "credentialImport.tokensHint")}</FieldDescription>
          </Field>
        </FieldGroup>
      </fieldset>
      <p role="status" className="text-sm">{t("credentialImport.summary", { count: parsed.items.length, duplicates: parsed.duplicates, errors: parsed.errors.length })}</p>
      {parsed.errors.length ? <ul id={`${id}-errors`} className="max-h-32 overflow-auto text-sm text-destructive">{parsed.errors.map((error, index) => <li key={index}>{t("credentialImport.errorAt", { index: error.source, reason: t(`credentialImport.${error.code}`) })}</li>)}</ul> : null}
      {parsed.items.length ? <details><summary className="cursor-pointer py-2 text-sm">{t("credentialImport.preview")}</summary><ol className="max-h-40 overflow-auto text-sm">{parsed.items.map(item => <li key={item.source} className="break-all">{item.source}. {label(item)}</li>)}</ol></details> : null}
      <p className="text-sm text-muted-foreground">{t("credentialImport.policy")}</p>
      {ownership.error || save.error ? <ErrorNotice error={ownership.error ?? save.error} /> : null}
      <div className="flex flex-wrap justify-end gap-2"><Button type="button" variant="outline" disabled={save.isPending} onClick={onClose}>{t("actions.cancel")}</Button><Button type="submit" disabled={blocked}>{t("credentialImport.submit", { count: parsed.items.length })}</Button></div>
    </form>
  </ManagementDialog>
}

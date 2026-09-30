import { useId, useState, type FormEvent } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import {
  INFO_KEY,
  SETTINGS_KEY,
  instanceInfo,
  readSettings,
  saveSettings,
  vocabularies,
} from "@/api/settings"
import { ProxyControl, type ProxySettings } from "@/components/proxy-control"
import { SearchableSelect } from "@/components/searchable-select"
import { optionSource } from "@/api/options"
import { connectionProfiles } from "@/api/configuration"
import { navigate, useRoute } from "@/lib/router"
import { TransferPanel } from "@/pages/transfer"
import { SESSION_KEY, useConsoleContext } from "@/capability/session"
import type { SettingsDto } from "@/generated/sdk"
import { Page, PageHeader } from "@/components/page"
import { ErrorNotice, QueryState } from "@/components/state"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { StringList } from "@/pages/providers/config-controls"
import { groups, settingsPatch, type SettingField } from "@/pages/settings/schema"

export function SettingsPage() {
  const { t } = useTranslation()
  const context = useConsoleContext()
  const route = useRoute()
  const [settingsTab, setSettingsTab] = useState("general")
  const canTransfer = context.has("configuration.transfer")
  const tab = route === "/settings/transfer" && canTransfer ? "transfer" : settingsTab
  const setTab = (value: string) => {
    if (value !== "transfer") setSettingsTab(value)
    navigate(value === "transfer" ? "/settings/transfer" : "/settings", { replace: true })
  }
  const data = useQuery({ queryKey: SETTINGS_KEY, queryFn: readSettings })
  const info = useQuery({ queryKey: INFO_KEY, queryFn: instanceInfo })
  return (
    <Page>
      <PageHeader
        title={t("nav.settings")}
        actions={
          info.data ? (
            <span className="flex items-center gap-2">
              <Badge variant="outline">v{info.data.version}</Badge>
              <code title={info.data.hash}>{info.data.hash.slice(0, 12)}</code>
            </span>
          ) : null
        }
      />
      <Tabs value={tab} onValueChange={setTab} className="gap-6">
        <TabsList variant="line" className="max-w-full">
          {groups.map((group) => (
            <TabsTrigger key={group.id} value={group.id}>
              {t(`settingsGroup.${group.id}`)}
            </TabsTrigger>
          ))}
          {canTransfer ? <TabsTrigger value="transfer">{t("nav.transfer")}</TabsTrigger> : null}
        </TabsList>
        <div hidden={tab === "transfer"}>
          <QueryState isPending={data.isPending} error={data.error}>
            {data.data ? <SettingsForm key={data.data.instance.configRevision} original={data.data} /> : null}
          </QueryState>
        </div>
        {canTransfer ? <TabsContent value="transfer"><TransferPanel /></TabsContent> : null}
      </Tabs>
    </Page>
  )
}

function SettingsForm({ original }: { original: SettingsDto }) {
  const { t } = useTranslation()
  const id = useId()
  const client = useQueryClient()
  const [draft, setDraft] = useState(original)
  const [resetEpoch, setResetEpoch] = useState(0)
  const [token, setToken] = useState<string | null | undefined>(undefined)
  const files = useQuery({ queryKey: ["configuration", "vocabularies"], queryFn: vocabularies })
  const patch = settingsPatch(original, draft, token)
  const changed = Object.keys(patch).length > 0
  const saved = useMutation({
    mutationFn: saveSettings,
    onSuccess: (value) => {
      client.setQueryData(SETTINGS_KEY, value)
      void client.invalidateQueries({ queryKey: INFO_KEY })
      void client.invalidateQueries({ queryKey: SESSION_KEY })
      toast.success(t("toast.saved"))
    },
  })
  const change = (field: SettingField, value: unknown) =>
    setDraft((previous) => ({
      ...previous,
      [field.group]: { ...previous[field.group], [field.name]: value },
    }))
  const submit = (event: FormEvent) => {
    event.preventDefault()
    if (changed) saved.mutate(patch)
  }
  function control(field: SettingField) {
    const value = (draft[field.group] as Record<string, unknown>)[field.name]
    const inputId = `${id}-${field.name}`
    if (field.kind === "proxy") return <ProxyControl id={inputId} value={value as ProxySettings} onChange={v => change(field, v)} scope={{ scope: "global" }} />
    if (field.kind === "switch")
      return <Switch id={inputId} checked={Boolean(value)} onCheckedChange={(v) => change(field, v)} />
    if (field.kind === "list")
      return <StringList value={value as Array<string>} onChange={(v) => change(field, v)} />
    if (field.kind === "allowlist")
      return (
        <div className="flex flex-col gap-3">
          <Select
            value={value === null ? "all" : "limited"}
            onValueChange={(v) => change(field, v === "all" ? null : [])}
          >
            <SelectTrigger id={inputId}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                <SelectItem value="all">{t("settings.allClients")}</SelectItem>
                <SelectItem value="limited">{t("settings.selectedClients")}</SelectItem>
              </SelectGroup>
            </SelectContent>
          </Select>
          {value !== null ? (
            <StringList key="limited" value={value as Array<string>} onChange={(v) => change(field, v)} />
          ) : null}
        </div>
      )
    if (field.kind === "profile") return <SearchableSelect id={inputId} label={t(`setting.${field.name}`)} value={value == null ? "" : String(value)} onChange={v => change(field, v || null)} emptyLabel={t("settings.default")} source={optionSource(connectionProfiles, row => ({ value: row.id, label: row.name }))} />
    if (["choice", "vocabulary"].includes(field.kind)) {
      const options =
        field.kind === "vocabulary"
            ? (files.data?.map((row) => ({ value: row.fileId, label: row.filename ?? row.fileId })) ?? [])
            : field.options!.map((option) => ({ value: option, label: t(`settingsOption.${option}`) }))
      const error =
        field.kind === "vocabulary" ? files.error : null
      return (
        <>
          <Select
            value={value == null ? "__default" : String(value)}
            onValueChange={(v) => change(field, v === "__default" ? null : v)}
          >
            <SelectTrigger id={inputId} className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                {field.nullable ? <SelectItem value="__default">{t("settings.default")}</SelectItem> : null}
                {options.map((o) => (
                  <SelectItem key={o.value} value={o.value}>
                    {o.label}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
          {error ? <ErrorNotice error={error} /> : null}
        </>
      )
    }
    return (
      <Input
        id={inputId}
        type={field.kind === "number" ? "number" : "text"}
        step={1}
        min={field.min}
        required={!field.nullable}
        placeholder={field.nullable ? t("settings.unset") : undefined}
        value={value == null ? "" : String(value)}
        onChange={(e) =>
          change(
            field,
            field.kind === "number"
              ? e.target.value === ""
                ? field.nullable
                  ? null
                  : ""
                : Number(e.target.value)
              : e.target.value,
          )
        }
      />
    )
  }
  return (
    <form onSubmit={submit} className="flex min-w-0 flex-col gap-6">
      {saved.error ? <ErrorNotice error={saved.error} /> : null}
      {groups.map((group) => (
        <TabsContent key={group.id} value={group.id} forceMount className="data-[state=inactive]:hidden">
          <FieldGroup key={resetEpoch} className="grid gap-6 sm:grid-cols-2">
            {group.fields.map((field) => (
              <Field
                key={field.name}
                className={["list", "allowlist"].includes(field.kind) ? "sm:col-span-2" : undefined}
              >
                <FieldLabel htmlFor={`${id}-${field.name}`}>{t(`setting.${field.name}`)}</FieldLabel>
                {control(field)}
                {field.name === "allowedHeaders" ? <FieldDescription>{t("setting.allowedHeadersHelp")}</FieldDescription> : null}
              </Field>
            ))}
            {group.id === "tokenizer" ? (
              <Field className="sm:col-span-2">
                <FieldLabel htmlFor={`${id}-token`}>
                  {t("setting.tokenizerAuthToken")}
                  <Badge variant="outline">
                    {original.instance.hasTokenizerAuthToken && token !== null
                      ? t("settings.configured")
                      : t("settings.notConfigured")}
                  </Badge>
                </FieldLabel>
                <div className="flex gap-2">
                  <Input
                    id={`${id}-token`}
                    type="password"
                    autoComplete="new-password"
                    value={token ?? ""}
                    onChange={(e) => setToken(e.target.value || undefined)}
                  />
                  <Button type="button" variant="outline" onClick={() => setToken(null)}>
                    {t("settings.clearToken")}
                  </Button>
                </div>
              </Field>
            ) : null}
            {group.id === "maintenance" ? (
              <p className="sm:col-span-2 text-sm">{t("settings.cleanupScope")}</p>
            ) : null}
          </FieldGroup>
        </TabsContent>
      ))}
      <div className="sticky bottom-0 flex justify-end gap-2 border-t bg-background py-3">
        <Button
          type="button"
          variant="outline"
          disabled={!changed || saved.isPending}
          onClick={() => {
            setDraft(original)
            setToken(undefined)
            setResetEpoch((v) => v + 1)
          }}
        >
          {t("actions.cancel")}
        </Button>
        <Button type="submit" disabled={!changed || saved.isPending}>
          {t("actions.save")}
        </Button>
      </div>
    </form>
  )
}

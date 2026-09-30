import { useId, useState, type FormEvent } from "react"
import { RotateCcw } from "lucide-react"
import { useTranslation } from "react-i18next"
import type { ChannelDescriptor, ProviderDto, ProviderWrite } from "@/generated/sdk"
import { SearchableSelect } from "@/components/searchable-select"
import { optionSource } from "@/api/options"
import { connectionProfiles } from "@/api/configuration"
import { ProxyControl, type ProxySettings } from "@/components/proxy-control"
import { ErrorNotice } from "@/components/state"
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
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { ConfigControl } from "@/pages/providers/config-controls"
import { controlFor, setConfigValue, configPlaceholder, type ConfigObject } from "@/pages/providers/config-schema"

export type ProviderFormProps = {
  catalog: Array<ChannelDescriptor>
  provider?: ProviderDto
  onSubmit: (body: Partial<ProviderWrite>) => void
  pending?: boolean
  error?: unknown
  onCancel?: () => void
}

export function ProviderForm({ catalog, provider, onSubmit, pending, error, onCancel }: ProviderFormProps) {
  const { t } = useTranslation()
  const id = useId()
  const [channel, setChannel] = useState(provider?.channel ?? catalog[0]?.id ?? "")
  const [name, setName] = useState(provider?.name ?? channel.toLowerCase().replaceAll("_", ""))
  const [displayName, setDisplayName] = useState(provider?.displayName ?? (provider ? "" : catalog[0]?.displayName ?? ""))
  const changeChannel = (next: string) => {
    setName(current => !current || current === channel.toLowerCase().replaceAll("_", "") ? next.toLowerCase().replaceAll("_", "") : current)
    setDisplayName(current => !current || current === catalog.find(item => item.id === channel)?.displayName ? catalog.find(item => item.id === next)?.displayName ?? "" : current)
    setChannel(next)
  }
  const [baseUrl, setBaseUrl] = useState(provider?.baseUrl ?? "")
  const [profile, setProfile] = useState(provider?.connectionProfileId ?? "__default")
  const [proxy, setProxy] = useState<ProxySettings>(provider?.proxy ?? null)
  const [enabled, setEnabled] = useState(provider?.enabled ?? true)
  const [configs, setConfigs] = useState<Record<string, ConfigObject>>({
    [channel]: (provider?.config ?? {}) as ConfigObject,
  })
  const [resets, setResets] = useState<Record<string, number>>({})
  const descriptor = catalog.find((item) => item.id === channel)
  const hiddenKeys = ["user_agent", "client_id", "client_secret", "oauth_client_id", "sso_client_id", "sso_client_secret", ...(["antigravity", "geminicli"].includes(channel) ? ["project_id"] : [])]
  const config = Object.fromEntries(Object.entries(configs[channel] ?? {}).filter(([key]) => !hiddenKeys.includes(key)))
  const fields = descriptor?.configKeys.filter((field) => field.name !== "base_url" && !hiddenKeys.includes(field.name)) ?? []
  const change = (key: string, value: unknown) =>
    setConfigs((previous) => ({
      ...previous,
      [channel]: setConfigValue(previous[channel] ?? {}, key, value),
    }))
  const submit = (event: FormEvent) => {
    event.preventDefault()
    onSubmit({
      name: name.trim(),
      displayName: displayName.trim() || null,
      ...(provider ? {} : { channel }),
      baseUrl: baseUrl.trim() || null,
      connectionProfileId: profile === "__default" ? null : profile,
      proxy,
      enabled,
      config,
    })
  }
  return (
    <form onSubmit={submit} className="flex min-w-0 flex-col gap-6">
      {error ? <ErrorNotice error={error} /> : null}
      <FieldGroup className="grid gap-5 sm:grid-cols-2">
        <Field>
          <FieldLabel htmlFor={`${id}-name`}>{t("providerForm.routeName")}</FieldLabel>
          <Input id={`${id}-name`} required value={name} onChange={(e) => setName(e.target.value)} />
        </Field>
        <Field>
          <FieldLabel htmlFor={`${id}-display-name`}>{t("providerForm.displayName")}</FieldLabel>
          <Input id={`${id}-display-name`} value={displayName} placeholder={name} onChange={event => setDisplayName(event.target.value)} />
        </Field>
        <Field>
          <FieldLabel htmlFor={`${id}-channel`}>{t("fields.channel")}</FieldLabel>
          <Select value={channel} onValueChange={changeChannel} disabled={Boolean(provider)}>
            <SelectTrigger id={`${id}-channel`} className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                {catalog.map((entry) => (
                  <SelectItem key={entry.id} value={entry.id}>
                    {entry.displayName}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        </Field>
        <Field>
          <FieldLabel htmlFor={`${id}-url`}>{t("fields.baseUrl")}</FieldLabel>
          <Input
            id={`${id}-url`}
            type="url"
            required={descriptor?.configKeys.find((field) => field.name === "base_url")?.required}
            value={baseUrl}
            placeholder={configPlaceholder(descriptor?.configKeys.find(field => field.name === "base_url"), descriptor?.configKeys ?? [], config, baseUrl) ?? (descriptor?.configKeys.find(field => field.name === "base_url")?.required ? undefined : t("providerForm.dynamicAddress"))}
            onChange={(e) => setBaseUrl(e.target.value)}
          />
        </Field>
        <Field>
          <FieldLabel htmlFor={`${id}-profile`}>{t("providerForm.connection")}</FieldLabel>
          <SearchableSelect id={`${id}-profile`} label={t("providerForm.connection")} value={profile} onChange={setProfile} emptyValue="__default" emptyLabel={t("form.unset")} source={optionSource(connectionProfiles, row => ({ value: row.id, label: row.name }))} />
        </Field>
        <Field data-field-span="full"><FieldLabel htmlFor={`${id}-proxy`}>{t("proxy.provider")}</FieldLabel><ProxyControl id={`${id}-proxy`} value={proxy} onChange={setProxy} scope={provider ? { scope: "provider", provider_id: provider.id } : { scope: "global", parent: true }} /></Field>
        <Field>
          <FieldLabel htmlFor={`${id}-auto-refresh-models`}>{t("providerForm.autoRefreshModels")}</FieldLabel>
          <Switch id={`${id}-auto-refresh-models`} checked={config.auto_refresh_models !== false} onCheckedChange={value => change("auto_refresh_models", value)} />
          <FieldDescription>{t("providerForm.autoRefreshModelsHelp")}</FieldDescription>
        </Field>
        <Field>
          <FieldLabel htmlFor={`${id}-enabled`}>{t("fields.enabled")}</FieldLabel>
          <Switch id={`${id}-enabled`} checked={enabled} onCheckedChange={setEnabled} />
        </Field>
      </FieldGroup>
      <FieldGroup key={channel} className="grid gap-5 border-t pt-6 sm:grid-cols-2">
        {fields.map((field) => (
          <Field
            key={`${field.name}-${resets[field.name] ?? 0}`}
            className={
              ["list", "pairs", "dialects", "routing", "model-routing"].includes(controlFor(field))
                ? "sm:col-span-2"
                : undefined
            }
          >
            <div className="flex items-center justify-between gap-2">
              <FieldLabel htmlFor={`${id}-${field.name}`}>{t(`providerConfig.${field.name}`)}</FieldLabel>
              {!field.required && config[field.name] !== undefined ? (
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-sm"
                  aria-label={t("providerForm.reset", { name: t(`providerConfig.${field.name}`) })}
                  title={t("providerForm.default")}
                  onClick={() => {
                    change(field.name, undefined)
                    setResets((previous) => ({ ...previous, [field.name]: (previous[field.name] ?? 0) + 1 }))
                  }}
                >
                  <RotateCcw />
                </Button>
              ) : null}
            </div>
            <ConfigControl
              id={`${id}-${field.name}`}
              field={field}
              placeholder={configPlaceholder(field, descriptor?.configKeys ?? [], config, baseUrl)}
              value={field.name === "session_affinity"
                ? config.session_affinity ?? ["sticky", "round_robin_affinity"].includes(String(config.credential_strategy))
                : config[field.name]}
              onChange={(value) => change(field.name, value)}
            />
          </Field>
        ))}
      </FieldGroup>
      <div className="sticky bottom-0 flex justify-end gap-2 border-t bg-background py-3">
        {onCancel ? (
          <Button type="button" variant="outline" onClick={onCancel}>
            {t("actions.cancel")}
          </Button>
        ) : null}
        <Button type="submit" disabled={pending}>
          {provider ? t("actions.save") : t("actions.create")}
        </Button>
      </div>
    </form>
  )
}

export function ProviderDialog({
  open,
  onOpenChange,
  ...props
}: ProviderFormProps & { open: boolean; onOpenChange: (open: boolean) => void }) {
  const { t } = useTranslation()
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-3xl" closeLabel={t("actions.close")} aria-describedby={undefined}>
        <DialogHeader>
          <DialogTitle>{props.provider ? t("edit.providers") : t("create.providers")}</DialogTitle>
        </DialogHeader>
        <div className="min-h-0 overflow-y-auto px-4">
          {open ? <ProviderForm {...props} onCancel={() => onOpenChange(false)} /> : null}
        </div>
      </DialogContent>
    </Dialog>
  )
}

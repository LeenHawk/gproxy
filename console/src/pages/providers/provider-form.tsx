import { useId, useState, type FormEvent } from "react"
import { RotateCcw } from "lucide-react"
import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import type { ChannelDescriptor, ProviderDto, ProviderWrite } from "@/generated/sdk"
import { connectionProfiles } from "@/api/configuration"
import { ErrorNotice, QueryState } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
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
import { controlFor, setConfigValue, type ConfigObject } from "@/pages/providers/config-schema"

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
  const [name, setName] = useState(provider?.name ?? "")
  const [channel, setChannel] = useState(provider?.channel ?? catalog[0]?.id ?? "")
  const [baseUrl, setBaseUrl] = useState(provider?.baseUrl ?? "")
  const [profile, setProfile] = useState(provider?.connectionProfileId ?? "__default")
  const [enabled, setEnabled] = useState(provider?.enabled ?? true)
  const [configs, setConfigs] = useState<Record<string, ConfigObject>>({
    [channel]: (provider?.config ?? {}) as ConfigObject,
  })
  const [resets, setResets] = useState<Record<string, number>>({})
  const profiles = useQuery({
    queryKey: ["configuration", "connection-profiles"],
    queryFn: connectionProfiles,
  })
  const descriptor = catalog.find((item) => item.id === channel)
  const config = configs[channel] ?? {}
  const fields = descriptor?.configKeys.filter((field) => field.name !== "base_url") ?? []
  const change = (key: string, value: unknown) =>
    setConfigs((previous) => ({
      ...previous,
      [channel]: setConfigValue(previous[channel] ?? {}, key, value),
    }))
  const submit = (event: FormEvent) => {
    event.preventDefault()
    onSubmit({
      name: name.trim(),
      channel,
      baseUrl: baseUrl.trim() || null,
      connectionProfileId: profile === "__default" ? null : profile,
      enabled,
      config,
    })
  }
  return (
    <form onSubmit={submit} className="flex min-w-0 flex-col gap-6">
      {error ? <ErrorNotice error={error} /> : null}
      <FieldGroup className="grid gap-5 sm:grid-cols-2">
        <Field>
          <FieldLabel htmlFor={`${id}-name`}>{t("fields.name")}</FieldLabel>
          <Input id={`${id}-name`} required value={name} onChange={(e) => setName(e.target.value)} />
        </Field>
        <Field>
          <FieldLabel htmlFor={`${id}-channel`}>{t("fields.channel")}</FieldLabel>
          <Select value={channel} onValueChange={setChannel}>
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
            placeholder={t("providerForm.default")}
            onChange={(e) => setBaseUrl(e.target.value)}
          />
        </Field>
        <Field>
          <FieldLabel htmlFor={`${id}-profile`}>{t("providerForm.connection")}</FieldLabel>
          <QueryState isPending={profiles.isPending} error={profiles.error}>
            <Select value={profile} onValueChange={setProfile}>
              <SelectTrigger id={`${id}-profile`} className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  <SelectItem value="__default">{t("providerForm.default")}</SelectItem>
                  {profiles.data?.map((row) => (
                    <SelectItem key={row.id} value={row.id}>
                      {row.name}
                    </SelectItem>
                  ))}
                </SelectGroup>
              </SelectContent>
            </Select>
          </QueryState>
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
              value={config[field.name]}
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
        <Button type="submit" disabled={pending || profiles.isPending || !!profiles.error}>
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

import type { InstanceSettingsDto } from "@/generated/InstanceSettingsDto"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { saveInstanceSettings } from "@/api/control"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Field, FieldContent, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"

type PreferenceChange =
  | { kind: "channel"; value: InstanceSettingsDto["update_channel"] }
  | { kind: "automatic"; value: boolean }

const UPDATE_CHANNELS = ["dev", "releases", "staging"] satisfies ReadonlyArray<NonNullable<InstanceSettingsDto["update_channel"]>>

export function UpdatePreferences({ settings }: { settings: InstanceSettingsDto }) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const save = useMutation({
    mutationFn: (change: PreferenceChange) => saveInstanceSettings({
      ...settings,
      ...(change.kind === "channel"
        ? { update_channel: change.value }
        : { enable_auto_update_check: change.value }),
    }),
    onSuccess: (value, change) => {
      queryClient.setQueryData(["instance-settings"], value)
      if (change.kind === "channel") queryClient.removeQueries({ queryKey: ["native-update"] })
      toast.success(t(change.kind === "channel" ? "update.preferences.channelSaved" : "update.preferences.automaticSaved"))
    },
    onError: () => toast.error(t("update.preferences.saveError")),
  })
  const channel = save.isPending && save.variables.kind === "channel"
    ? save.variables.value
    : settings.update_channel
  const automatic = save.isPending && save.variables.kind === "automatic"
    ? save.variables.value
    : settings.enable_auto_update_check

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("update.preferences.title")}</CardTitle>
        <CardDescription>{t("update.preferences.description")}</CardDescription>
      </CardHeader>
      <CardContent>
        <FieldGroup className="sm:grid-cols-1">
          <Field>
            <FieldLabel htmlFor="update-channel">{t("update.preferences.channel")}</FieldLabel>
            <Select
              value={channel ?? "default"}
              disabled={save.isPending}
              onValueChange={(value) => {
                const channel = value === "default" ? null : UPDATE_CHANNELS.find((item) => item === value)
                if (channel !== undefined) save.mutate({ kind: "channel", value: channel })
              }}
            >
              <SelectTrigger id="update-channel" className="w-full max-w-sm"><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  <SelectItem value="default">{t("update.preferences.channels.default")}</SelectItem>
                  <SelectItem value="dev">{t("update.preferences.channels.dev")}</SelectItem>
                  <SelectItem value="releases">{t("update.preferences.channels.releases")}</SelectItem>
                  <SelectItem value="staging">{t("update.preferences.channels.staging")}</SelectItem>
                </SelectGroup>
              </SelectContent>
            </Select>
            <FieldDescription>{t("update.preferences.channelHint")}</FieldDescription>
          </Field>
          <Field orientation="horizontal" data-disabled={save.isPending || undefined}>
            <FieldContent>
              <FieldLabel htmlFor="automatic-update-check">{t("update.preferences.automatic")}</FieldLabel>
              <FieldDescription>{t("update.preferences.automaticHint")}</FieldDescription>
            </FieldContent>
            <Switch
              id="automatic-update-check"
              checked={automatic}
              disabled={save.isPending}
              onCheckedChange={(value) => save.mutate({ kind: "automatic", value })}
            />
          </Field>
        </FieldGroup>
      </CardContent>
    </Card>
  )
}

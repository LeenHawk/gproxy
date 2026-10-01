import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { SHELL_PREFERENCES_KEY, ohosAction, setOhosBackground, type OhosAction, type OhosStatus, type ShellPreferencesStatus } from "@/api/preferences"
import { ErrorNotice } from "@/components/state"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Field, FieldContent, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Switch } from "@/components/ui/switch"

export function OhosSettings({ original }: { original: OhosStatus }) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const action = useMutation({
    mutationFn: (value: OhosAction | boolean) => typeof value === "boolean" ? setOhosBackground(value) : ohosAction(value),
    onSuccess: ohos => client.setQueryData<ShellPreferencesStatus>(SHELL_PREFERENCES_KEY, previous => previous ? { ...previous, ohos } : previous),
  })
  return <div className="flex flex-col gap-6">
    <FieldGroup>
      <Field>
        <FieldLabel>{t("shellPreferences.ohosAutoStart")}</FieldLabel>
        <div className="flex flex-wrap items-center gap-3">
          <Badge variant="outline">{t(original.autoStart === null ? "shellPreferences.unknown" : original.autoStart ? "shellPreferences.enabled" : "shellPreferences.disabled")}</Badge>
          <Button variant="outline" disabled={action.isPending} onClick={() => action.mutate("startup-settings")}>{t("shellPreferences.openStartupSettings")}</Button>
        </div>
        <FieldDescription>{t("shellPreferences.ohosStartup")}</FieldDescription>
        {original.startupError ? <FieldDescription>{t("shellPreferences.startupUnknown")}</FieldDescription> : null}
      </Field>
      {original.canMinimize ? <>
        {original.canTray ? <>
        <Field orientation="horizontal">
          <FieldContent><FieldLabel htmlFor="ohos-tray">{t("shellPreferences.tray")}</FieldLabel><FieldDescription id="ohos-tray-description-0">{t("shellPreferences.trayHelp")}</FieldDescription></FieldContent>
          <Switch aria-describedby="ohos-tray-description-0" id="ohos-tray" checked={original.tray} disabled={action.isPending} onCheckedChange={enabled => action.mutate(enabled ? "tray-on" : "tray-off")} />
        </Field>
        <Field orientation="horizontal" data-disabled={!original.trayReady || !original.active || action.isPending}>
          <FieldContent><FieldLabel htmlFor="ohos-close">{t("shellPreferences.ohosClose")}</FieldLabel><FieldDescription id="ohos-close-description-0">{t("shellPreferences.ohosCloseHelp")}</FieldDescription></FieldContent>
          <Switch aria-describedby="ohos-close-description-0" id="ohos-close" checked={original.closeToTray} disabled={!original.trayReady || !original.active || action.isPending} onCheckedChange={enabled => action.mutate(enabled ? "close-hide" : "close-quit")} />
        </Field>
        </> : null}
        <Field orientation="horizontal">
          <FieldContent><FieldLabel htmlFor="ohos-minimized">{t("shellPreferences.ohosMinimized")}</FieldLabel><FieldDescription id="ohos-minimized-description-0">{t("shellPreferences.ohosMinimizedHelp")}</FieldDescription></FieldContent>
          <Switch aria-describedby="ohos-minimized-description-0" id="ohos-minimized" checked={original.startMinimized} disabled={action.isPending} onCheckedChange={enabled => action.mutate(enabled ? "minimize-on" : "minimize-off")} />
        </Field>
      </> : null}
      <Field orientation="horizontal" data-disabled={!original.supported || action.isPending}>
        <FieldContent>
          <FieldLabel htmlFor="ohos-background">{t("shellPreferences.ohosBackground")}</FieldLabel>
          <FieldDescription id="ohos-background-description-0">{t(original.supported ? "shellPreferences.ohosHelp" : "shellPreferences.ohosUnsupported")}</FieldDescription>
          {original.supported ? <FieldDescription id="ohos-background-description-1">{t(original.active ? "shellPreferences.ohosActive" : "shellPreferences.ohosInactive")}</FieldDescription> : null}
        </FieldContent>
        <Switch aria-describedby="ohos-background-description-0 ohos-background-description-1" id="ohos-background" checked={original.active} disabled={!original.supported || action.isPending} onCheckedChange={enabled => action.mutate(enabled)} />
      </Field>
    </FieldGroup>
    {original.cancelled ? <Alert><AlertDescription>{t("shellPreferences.ohosCancelled")}</AlertDescription></Alert> : null}
    {original.trayError ? <Alert><AlertDescription>{t("shellPreferences.trayFailed", { error: original.trayError })}</AlertDescription></Alert> : null}
    {original.error ? <ErrorNotice error={new Error(original.error)} /> : null}
    {action.error ? <ErrorNotice error={action.error} /> : null}
  </div>
}

import { useId } from "react"
import { useTranslation } from "react-i18next"
import type { ShellPreferences } from "@/api/preferences"
import { Field, FieldContent, FieldDescription, FieldLabel } from "@/components/ui/field"
import { Switch } from "@/components/ui/switch"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"

export function ShellPreferenceFields({ value, onChange, desktop, canAutoStart, disabled }: {
  value: ShellPreferences; onChange: (value: ShellPreferences) => void
  desktop: boolean; canAutoStart: boolean; disabled?: boolean
}) {
  const { t } = useTranslation()
  const id = useId()
  return <>
    {canAutoStart ? <Field orientation="horizontal" className="sm:col-span-2" data-disabled={disabled}>
      <FieldContent><FieldLabel htmlFor={`${id}-startup`}>{t(desktop ? "setup.autoStart" : "setup.androidAutoStart")}</FieldLabel></FieldContent>
      <Switch id={`${id}-startup`} checked={value.autoStart} disabled={disabled} onCheckedChange={autoStart => onChange({ ...value, autoStart })} />
    </Field> : null}
    {desktop ? <>
      <Field orientation="horizontal" className="sm:col-span-2" data-disabled={disabled}>
        <FieldContent><FieldLabel htmlFor={`${id}-tray`}>{t("shellPreferences.tray")}</FieldLabel><FieldDescription id={(`${id}-tray`) + "-description-0"}>{t("shellPreferences.trayHelp")}</FieldDescription></FieldContent>
        <Switch aria-describedby={[(`${id}-tray`) + "-description-0"].join(" ")} id={`${id}-tray`} checked={value.tray} disabled={disabled} onCheckedChange={tray => onChange({ ...value, tray, closeToTray: tray && value.closeToTray })} />
      </Field>
      <Field data-disabled={disabled || !value.tray}>
        <FieldLabel htmlFor={`${id}-close`}>{t("shellPreferences.close")}</FieldLabel>
        <Select value={value.tray && value.closeToTray ? "background" : "quit"} disabled={disabled || !value.tray} onValueChange={action => onChange({ ...value, closeToTray: action === "background" })}>
          <SelectTrigger id={`${id}-close`}><SelectValue /></SelectTrigger>
          <SelectContent><SelectGroup><SelectItem value="background">{t("shellPreferences.background")}</SelectItem><SelectItem value="quit">{t("shellPreferences.quit")}</SelectItem></SelectGroup></SelectContent>
        </Select>
      </Field>
      <Field orientation="horizontal" data-disabled={disabled || !value.autoStart || !value.tray}>
        <FieldContent><FieldLabel htmlFor={`${id}-hidden`}>{t("shellPreferences.hidden")}</FieldLabel><FieldDescription id={(`${id}-hidden`) + "-description-0"}>{t("shellPreferences.hiddenHelp")}</FieldDescription></FieldContent>
        <Switch aria-describedby={[(`${id}-hidden`) + "-description-0"].join(" ")} id={`${id}-hidden`} checked={value.autoStart && value.tray && value.startHidden} disabled={disabled || !value.autoStart || !value.tray} onCheckedChange={startHidden => onChange({ ...value, startHidden })} />
      </Field>
    </> : null}
  </>
}

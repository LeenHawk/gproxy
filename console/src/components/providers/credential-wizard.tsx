import type { ChannelDto } from "@/generated/ChannelDto"
import type { LoginModeDto } from "@/generated/LoginModeDto"
import { useQueryClient } from "@tanstack/react-query"
import { useId, useState } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { AuthcodeFlow } from "@/components/providers/authcode-flow"
import { CookieFlow } from "@/components/providers/cookie-flow"
import { DeviceFlow } from "@/components/providers/device-flow"
import { loginParams, loginParamValues } from "@/components/providers/login-param-values"
import { LoginParams } from "@/components/providers/login-params"
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"

type Props = { providerId: number; channel: ChannelDto; onDone: () => void }

export function CredentialWizard({ providerId, channel, onDone }: Props) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const id = useId()
  const login = channel.login
  const modes = login?.modes ?? []
  const [mode, setMode] = useState<LoginModeDto>(modes[0] ?? "authcode")
  const [label, setLabel] = useState("")
  const [values, setValues] = useState(() => loginParamValues(login?.params ?? []))

  if (!login || !modes.length) return null

  const done = () => {
    void Promise.all([
      queryClient.invalidateQueries({ queryKey: ["credentials"] }),
      queryClient.invalidateQueries({ queryKey: ["credential-cycles"] }),
    ])
    toast.success(t("providers.credentials.created"))
    onDone()
  }
  const params = loginParams(values)
  const paramsReady = login.params
    .filter((param) => param.required && (!param.modes.length || param.modes.includes(mode)))
    .every((param) => Boolean(values[param.name]?.trim()))

  return (
    <div className="flex flex-col gap-5">
      {modes.length > 1 ? (
        <Field>
          <FieldLabel>{t("providers.login.method")}</FieldLabel>
          <ToggleGroup
            type="single"
            variant="outline"
            value={mode}
            onValueChange={(value) => { if (value) setMode(value as LoginModeDto) }}
          >
            {modes.map((item) => (
              <ToggleGroupItem key={item} value={item} aria-label={t(`providers.login.modes.${item}`)}>
                {t(`providers.login.modes.${item}`)}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
        </Field>
      ) : null}
      <Field>
        <FieldLabel htmlFor={`${id}-label`}>{t("providers.credentials.label")}</FieldLabel>
        <Input id={`${id}-label`} value={label} onChange={(event) => setLabel(event.target.value)} />
        <FieldDescription>{t("providers.login.labelHint")}</FieldDescription>
      </Field>
      <LoginParams
        mode={mode}
        params={login.params}
        values={values}
        onChange={(name, value) => setValues((current) => ({ ...current, [name]: value }))}
      />
      {mode === "authcode" ? <AuthcodeFlow providerId={providerId} label={label} params={params} disabled={!paramsReady} onDone={done} /> : null}
      {mode === "device" ? <DeviceFlow providerId={providerId} label={label} params={params} disabled={!paramsReady} onDone={done} /> : null}
      {mode === "cookie" ? <CookieFlow providerId={providerId} label={label} onDone={done} /> : null}
    </div>
  )
}

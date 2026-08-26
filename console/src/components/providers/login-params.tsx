import type { LoginModeDto } from "@/generated/LoginModeDto"
import type { LoginParamDto } from "@/generated/LoginParamDto"
import { useId } from "react"
import { useTranslation } from "react-i18next"
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

type Props = {
  mode: LoginModeDto
  params: Array<LoginParamDto>
  values: Record<string, string>
  onChange: (name: string, value: string) => void
}

export function LoginParams({ mode, params, values, onChange }: Props) {
  const { t } = useTranslation()
  const id = useId()
  const visible = params.filter((param) => !param.modes.length || param.modes.includes(mode))
  if (!visible.length) return null

  return (
    <FieldGroup>
      {visible.map((param) => {
        const fieldId = `${id}-${param.name}`
        const label = t(`providers.login.params.${param.name}.label`)
        return (
          <Field key={param.name}>
            <FieldLabel htmlFor={fieldId}>{label}</FieldLabel>
            {param.kind === "select" ? (
              <Select value={values[param.name] ?? ""} onValueChange={(value) => onChange(param.name, value)}>
                <SelectTrigger id={fieldId} aria-label={label}><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    {param.options.map((option) => (
                      <SelectItem key={option} value={option}>
                        {t(`providers.login.params.${param.name}.options.${option}`)}
                      </SelectItem>
                    ))}
                  </SelectGroup>
                </SelectContent>
              </Select>
            ) : (
              <Input
                id={fieldId}
                value={values[param.name] ?? ""}
                required={param.required}
                onChange={(event) => onChange(param.name, event.target.value)}
              />
            )}
          </Field>
        )
      })}
    </FieldGroup>
  )
}

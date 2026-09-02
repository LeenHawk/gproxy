import type { LoginParamDto } from "@/generated/LoginParamDto"
import type { LoginModeDto } from "@/generated/LoginModeDto"

export function loginParamValues(params: Array<LoginParamDto>) {
  return Object.fromEntries(params.map((param) => [param.name, param.default_value ?? ""]))
}

export function loginParams(values: Record<string, string>) {
  return Object.fromEntries(Object.entries(values).filter(([, value]) => value.trim() !== ""))
}

export function loginParamApplies(
  param: LoginParamDto,
  mode: LoginModeDto,
  values: Record<string, string>,
) {
  return (!param.modes.length || param.modes.includes(mode))
    && (param.condition == null || values[param.condition.param] === param.condition.equals)
}

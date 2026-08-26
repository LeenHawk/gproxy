import type { LoginParamDto } from "@/generated/LoginParamDto"

export function loginParamValues(params: Array<LoginParamDto>) {
  return Object.fromEntries(params.map((param) => [param.name, param.default_value ?? ""]))
}

export function loginParams(values: Record<string, string>) {
  return Object.fromEntries(Object.entries(values).filter(([, value]) => value.trim() !== ""))
}

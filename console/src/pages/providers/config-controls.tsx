import { useId, useState } from "react"
import { useTranslation } from "react-i18next"
import { Plus, Trash2 } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Field, FieldLabel } from "@/components/ui/field"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
import {
  booleanDefault,
  choices,
  controlFor,
  dialects,
  setConfigValue,
  type ConfigObject,
} from "@/pages/providers/config-schema"
import type { ConfigKey } from "@/generated/sdk"

export function Choice({
  id,
  value,
  options,
  onChange,
}: {
  id: string
  value: string | undefined
  options: Array<string>
  onChange: (value: string | undefined) => void
}) {
  const { t } = useTranslation()
  return (
    <Select
      value={value ?? "__default"}
      onValueChange={(next) => onChange(next === "__default" ? undefined : next)}
    >
      <SelectTrigger id={id} className="w-full">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        <SelectGroup>
          <SelectItem value="__default">{t("providerForm.default")}</SelectItem>
          {options.map((option) => (
            <SelectItem key={option} value={option}>
              {t(`providerOption.${option}`, { defaultValue: option })}
            </SelectItem>
          ))}
        </SelectGroup>
      </SelectContent>
    </Select>
  )
}

export function StringList({
  value,
  onChange,
  options,
}: {
  value: Array<string>
  onChange: (value: Array<string>) => void
  options?: Array<string>
}) {
  const { t } = useTranslation()
  const [rows, setRows] = useState(value)
  const change = (next: Array<string>) => {
    setRows(next)
    onChange(next.filter((row) => row.trim()))
  }
  return (
    <div className="flex flex-col gap-2">
      {rows.map((row, index) => (
        <div key={index} className="flex items-center gap-2">
          {options ? (
            <Select
              value={row}
              onValueChange={(next) => change(rows.map((v, i) => (i === index ? next : v)))}
            >
              <SelectTrigger className="flex-1">
                <SelectValue placeholder={t("form.choose")} />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  {options.map((option) => (
                    <SelectItem key={option} value={option}>
                      {t(`providerOption.${option}`, { defaultValue: option })}
                    </SelectItem>
                  ))}
                </SelectGroup>
              </SelectContent>
            </Select>
          ) : (
            <Input
              aria-label={t("providerForm.item", { index: index + 1 })}
              value={row}
              onChange={(e) => change(rows.map((v, i) => (i === index ? e.target.value : v)))}
            />
          )}
          <Button
            type="button"
            size="icon-sm"
            variant="ghost"
            aria-label={t("actions.remove")}
            onClick={() => change(rows.filter((_, i) => i !== index))}
          >
            <Trash2 />
          </Button>
        </div>
      ))}
      <Button
        type="button"
        size="sm"
        variant="outline"
        className="self-start"
        onClick={() => setRows([...rows, ""])}
      >
        <Plus data-icon="inline-start" />
        {t("actions.add")}
      </Button>
    </div>
  )
}

export function StringPairs({
  value,
  onChange,
}: {
  value: Record<string, string>
  onChange: (value: Record<string, string>) => void
}) {
  const { t } = useTranslation()
  const [rows, setRows] = useState(Object.entries(value))
  const change = (next: Array<[string, string]>) => {
    setRows(next)
    onChange(Object.fromEntries(next.filter(([key]) => key.trim())))
  }
  return (
    <div className="flex flex-col gap-2">
      {rows.map(([key, value], index) => (
        <div key={index} className="flex items-start gap-2">
          <div className="grid min-w-0 flex-1 gap-2 sm:grid-cols-2">
            <Input
              required
              aria-label={t("providerForm.key")}
              placeholder={t("providerForm.key")}
              value={key}
              onChange={(e) => change(rows.map((row, i) => (i === index ? [e.target.value, value] : row)))}
            />
            <Input
              aria-label={t("providerForm.value")}
              placeholder={t("providerForm.value")}
              value={value}
              autoComplete="off"
              onChange={(e) => change(rows.map((row, i) => (i === index ? [key, e.target.value] : row)))}
            />
          </div>
          <Button
            type="button"
            size="icon-sm"
            variant="ghost"
            aria-label={t("actions.remove")}
            onClick={() => change(rows.filter((_, i) => i !== index))}
          >
            <Trash2 />
          </Button>
        </div>
      ))}
      <Button
        type="button"
        size="sm"
        variant="outline"
        className="self-start"
        onClick={() => setRows([...rows, ["", ""]])}
      >
        <Plus data-icon="inline-start" />
        {t("actions.add")}
      </Button>
    </div>
  )
}

const routingLists = ["order", "only", "ignore", "quantizations"]
const routingFlags = ["allow_fallbacks", "require_parameters", "zdr", "enforce_distillable_text"]

export function RoutingFields({
  value,
  onChange,
}: {
  value: ConfigObject
  onChange: (value: ConfigObject) => void
}) {
  const { t } = useTranslation()
  const id = useId()
  const set = (name: string, v: unknown) => onChange(setConfigValue(value, name, v))
  return (
    <div className="grid gap-5 sm:grid-cols-2">
      {routingLists.map((name) => (
        <Field key={name}>
          <FieldLabel>{t(`providerRouting.${name}`)}</FieldLabel>
          <StringList value={(value[name] ?? []) as Array<string>} onChange={(v) => set(name, v)} />
        </Field>
      ))}
      {routingFlags.map((name) => (
        <Field key={name}>
          <FieldLabel htmlFor={`${id}-${name}`}>{t(`providerRouting.${name}`)}</FieldLabel>
          <Choice
            id={`${id}-${name}`}
            value={value[name] == null ? undefined : String(value[name])}
            options={["true", "false"]}
            onChange={(v) => set(name, v === undefined ? undefined : v === "true")}
          />
        </Field>
      ))}
      <Field>
        <FieldLabel htmlFor={`${id}-collection`}>{t("providerRouting.data_collection")}</FieldLabel>
        <Choice
          id={`${id}-collection`}
          value={value.data_collection as string | undefined}
          options={["allow", "deny"]}
          onChange={(v) => set("data_collection", v)}
        />
      </Field>
      <Field>
        <FieldLabel htmlFor={`${id}-sort`}>{t("providerRouting.sort")}</FieldLabel>
        <Choice
          id={`${id}-sort`}
          value={
            typeof value.sort === "string"
              ? value.sort
              : ((value.sort as ConfigObject | undefined)?.by as string | undefined)
          }
          options={["price", "throughput", "latency", "exacto"]}
          onChange={(v) =>
            set(
              "sort",
              v === undefined
                ? undefined
                : typeof value.sort === "object" && value.sort !== null
                  ? { ...value.sort, by: v }
                  : v,
            )
          }
        />
      </Field>
      <Field>
        <FieldLabel htmlFor={`${id}-partition`}>{t("providerRouting.partition")}</FieldLabel>
        <Choice
          id={`${id}-partition`}
          value={
            (typeof value.sort === "object" && value.sort !== null
              ? (value.sort as ConfigObject).partition
              : undefined) as string | undefined
          }
          options={["model", "none"]}
          onChange={(v) =>
            set(
              "sort",
              setConfigValue(
                typeof value.sort === "string" ? { by: value.sort } : ((value.sort ?? {}) as ConfigObject),
                "partition",
                v,
              ),
            )
          }
        />
      </Field>
      {["preferred_min_throughput", "preferred_max_latency"].map((name) => (
        <Field key={name} className="sm:col-span-2">
          <FieldLabel>{t(`providerRouting.${name}`)}</FieldLabel>
          <Cutoff value={value[name]} onChange={(v) => set(name, v)} />
        </Field>
      ))}
      <fieldset className="grid gap-3 sm:col-span-2 sm:grid-cols-3">
        <legend className="mb-3 text-sm font-medium">{t("providerRouting.max_price")}</legend>
        {["prompt", "completion", "request", "image", "audio"].map((name) => (
          <Field key={name}>
            <FieldLabel htmlFor={`${id}-price-${name}`}>{t(`providerRouting.${name}`)}</FieldLabel>
            <Input
              id={`${id}-price-${name}`}
              type="number"
              min="0"
              step="any"
              value={String((value.max_price as ConfigObject | undefined)?.[name] ?? "")}
              onChange={(e) =>
                set(
                  "max_price",
                  setConfigValue(
                    (value.max_price ?? {}) as ConfigObject,
                    name,
                    e.target.value === "" ? undefined : e.target.value,
                  ),
                )
              }
            />
          </Field>
        ))}
      </fieldset>
    </div>
  )
}

function ModelRouting({
  value,
  onChange,
}: {
  value: Record<string, ConfigObject>
  onChange: (value: Record<string, ConfigObject>) => void
}) {
  const { t } = useTranslation()
  const [rows, setRows] = useState(Object.entries(value))
  const change = (next: Array<[string, ConfigObject]>) => {
    setRows(next)
    onChange(Object.fromEntries(next.filter(([name]) => name.trim())))
  }
  return (
    <div className="flex flex-col gap-6">
      {rows.map(([name, config], index) => (
        <fieldset key={index} className="flex min-w-0 flex-col gap-4 rounded-lg border p-4">
          <div className="flex items-center gap-2">
            <Input
              required
              aria-label={t("fields.upstreamName")}
              placeholder={t("fields.upstreamName")}
              value={name}
              onChange={(e) => change(rows.map((row, i) => (i === index ? [e.target.value, config] : row)))}
            />
            <Button
              type="button"
              size="icon-sm"
              variant="ghost"
              aria-label={t("actions.remove")}
              onClick={() => change(rows.filter((_, i) => i !== index))}
            >
              <Trash2 />
            </Button>
          </div>
          <RoutingFields
            value={config}
            onChange={(next) => change(rows.map((row, i) => (i === index ? [name, next] : row)))}
          />
        </fieldset>
      ))}
      <Button
        type="button"
        size="sm"
        variant="outline"
        className="self-start"
        onClick={() => setRows([...rows, ["", {}]])}
      >
        <Plus data-icon="inline-start" />
        {t("actions.add")}
      </Button>
    </div>
  )
}

export function ConfigControl({
  id,
  field,
  value,
  onChange,
}: {
  id: string
  field: ConfigKey
  value: unknown
  onChange: (value: unknown) => void
}) {
  if (field.name === "prompt")
    return (
      <Textarea
        id={id}
        value={String(value ?? "")}
        rows={4}
        onChange={(e) => onChange(e.target.value === "" ? undefined : e.target.value)}
      />
    )
  switch (controlFor(field)) {
    case "choice":
      return (
        <Choice
          id={id}
          value={value as string | undefined}
          options={choices[field.name]}
          onChange={onChange}
        />
      )
    case "bool":
      return (
        <Switch
          id={id}
          checked={value === undefined ? booleanDefault(field.name) : Boolean(value)}
          onCheckedChange={onChange}
        />
      )
    case "list":
      return <StringList value={(value ?? []) as Array<string>} onChange={onChange} />
    case "pairs":
      return <StringPairs value={(value ?? {}) as Record<string, string>} onChange={onChange} />
    case "dialects":
      return <DialectList value={(value ?? []) as Array<string>} onChange={onChange} />
    case "routing":
      return <RoutingFields value={(value ?? {}) as ConfigObject} onChange={onChange} />
    case "model-routing":
      return <ModelRouting value={(value ?? {}) as Record<string, ConfigObject>} onChange={onChange} />
    case "integer":
      return (
        <Input
          id={id}
          type="number"
          min="0"
          step="1"
          required={field.required}
          value={value === undefined ? "" : String(value)}
          onChange={(e) => onChange(e.target.value === "" ? undefined : Number(e.target.value))}
        />
      )
    default:
      return (
        <Input
          id={id}
          type={controlFor(field) === "password" ? "password" : "text"}
          autoComplete="off"
          required={field.required}
          value={String(value ?? "")}
          onChange={(e) => onChange(e.target.value === "" ? undefined : e.target.value)}
        />
      )
  }
}

function DialectList({
  value,
  onChange,
}: {
  value: Array<string>
  onChange: (value: Array<string>) => void
}) {
  return <StringList value={value} onChange={onChange} options={dialects} />
}

function Cutoff({ value, onChange }: { value: unknown; onChange: (value: unknown) => void }) {
  const { t } = useTranslation()
  const id = useId()
  const percentile = typeof value === "object" && value !== null
  return (
    <div className="flex flex-col gap-3">
      <Choice
        id={id}
        value={value == null ? undefined : percentile ? "percentiles" : "threshold"}
        options={["threshold", "percentiles"]}
        onChange={(mode) => onChange(mode === undefined ? undefined : mode === "percentiles" ? {} : 0)}
      />
      {value == null ? null : percentile ? (
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
          {["p50", "p75", "p90", "p99"].map((key) => (
            <Field key={key}>
              <FieldLabel htmlFor={`${id}-${key}`}>{key.toUpperCase()}</FieldLabel>
              <Input
                id={`${id}-${key}`}
                type="number"
                min="0"
                step="any"
                value={String((value as ConfigObject)[key] ?? "")}
                onChange={(e) =>
                  onChange(
                    setConfigValue(
                      value as ConfigObject,
                      key,
                      e.target.value === "" ? undefined : Number(e.target.value),
                    ),
                  )
                }
              />
            </Field>
          ))}
        </div>
      ) : (
        <Input
          aria-label={t("providerForm.value")}
          type="number"
          min="0"
          step="any"
          value={String(value)}
          onChange={(e) => onChange(e.target.value === "" ? undefined : Number(e.target.value))}
        />
      )}
    </div>
  )
}

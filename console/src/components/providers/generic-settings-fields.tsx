import type { ChannelSettingField } from "@/lib/channel-meta";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";

interface GenericSettingsFieldsProps {
  fields: readonly ChannelSettingField[];
  values: Record<string, unknown>;
  onChange: (values: Record<string, unknown>) => void;
}

const RESERVED_SETTING_KEYS = new Set([
  "base_url",
  "endpoints",
  "circuit_breaker",
  "auto_refresh_models",
  "request_allowlist",
]);

/** Shared controls own reserved keys; the first declaration owns every other key. */
export function genericSettingFields(
  fields: readonly ChannelSettingField[],
): ChannelSettingField[] {
  const seen = new Set(RESERVED_SETTING_KEYS);
  return fields.filter((field) => {
    if (seen.has(field.key)) return false;
    seen.add(field.key);
    return true;
  });
}

function fieldValue(
  field: ChannelSettingField,
  values: Record<string, unknown>,
): unknown {
  const value = values[field.key] === undefined ? field.default : values[field.key];
  return field.control === "boolean" && field.required === true && typeof value !== "boolean"
    ? false
    : value;
}

function inputValue(field: ChannelSettingField, value: unknown): string {
  if (field.control === "string_list" && Array.isArray(value)) {
    return value.filter((item): item is string => typeof item === "string").join(", ");
  }
  return typeof value === "string" || typeof value === "number" ? String(value) : "";
}

function serializedValue(field: ChannelSettingField, value: unknown): unknown | undefined {
  if (field.control === "boolean") return typeof value === "boolean" ? value : undefined;
  if (field.control === "integer") {
    if (typeof value === "number") return Number.isInteger(value) ? value : undefined;
    if (typeof value !== "string" || value.trim() === "") return undefined;
    const number = Number(value);
    return Number.isInteger(number) ? number : undefined;
  }
  if (field.control === "string_list") {
    const items = Array.isArray(value)
      ? value.filter((item): item is string => typeof item === "string")
      : typeof value === "string" ? value.split(",") : [];
    const normalized = items.map((item) => item.trim()).filter(Boolean);
    return normalized.length > 0 ? normalized : undefined;
  }
  if (typeof value !== "string" || value.trim() === "") return undefined;
  return value.trim();
}

export function assembleGenericSettings(
  base: Record<string, unknown>,
  values: Record<string, unknown>,
  fields: readonly ChannelSettingField[],
): Record<string, unknown> {
  const result = { ...base };
  for (const field of fields) {
    const value = serializedValue(field, fieldValue(field, values));
    if (value === undefined) delete result[field.key];
    else result[field.key] = value;
  }
  return result;
}

export function GenericSettingsFields({
  fields,
  values,
  onChange,
}: GenericSettingsFieldsProps) {
  return fields.map((field, index) => {
    const id = `sf-generic-${index}`;
    const label = field.label ?? field.key;
    const value = fieldValue(field, values);
    const update = (next: unknown) => onChange({ ...values, [field.key]: next });

    if (field.control === "boolean") {
      return (
        <div key={field.key} className="flex items-center justify-between gap-4">
          <Label htmlFor={id}>{label}</Label>
          <Switch
            id={id}
            checked={value === true}
            aria-required={field.required === true}
            onCheckedChange={update}
          />
        </div>
      );
    }

    return (
      <div key={field.key} className="grid gap-2">
        <Label htmlFor={id}>{label}</Label>
        <Input
          id={id}
          type={field.control === "url" ? "url" : field.control === "integer" ? "number" : "text"}
          step={field.control === "integer" ? 1 : undefined}
          required={field.required}
          value={inputValue(field, value)}
          placeholder={field.placeholder}
          onChange={(event) => update(event.target.value)}
        />
      </div>
    );
  });
}

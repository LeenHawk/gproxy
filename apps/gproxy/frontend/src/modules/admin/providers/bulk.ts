import type { CredentialQueryRow } from "../../../lib/types";
import {
  createEmptyCredentialFormState,
  secretValuesFromSecretJson
} from "./credentials";
import type {
  BulkCredentialImportEntry,
  ChannelCredentialSchema,
  CredentialBulkMode,
  CredentialFieldSchema,
  CredentialFieldValue
} from "./types";

function isObject(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === "object" && !Array.isArray(value);
}

function isKeyOnlySchema(schema: ChannelCredentialSchema): boolean {
  return schema.fields.length === 1 && schema.fields[0]?.key === "api_key";
}

function fieldByKey(
  schema: ChannelCredentialSchema,
  key: string
): CredentialFieldSchema | undefined {
  return schema.fields.find((field) => field.key === key);
}

function coerceFieldValue(
  field: CredentialFieldSchema,
  raw: unknown
): CredentialFieldValue {
  if (field.type === "boolean") {
    if (raw === true) {
      return true;
    }
    if (typeof raw === "string") {
      return raw.trim().toLowerCase() === "true";
    }
    return false;
  }

  if (field.type === "optional_boolean") {
    if (raw === true || raw === false) {
      return raw;
    }
    if (typeof raw === "string") {
      const normalized = raw.trim().toLowerCase();
      if (normalized === "true") {
        return true;
      }
      if (normalized === "false") {
        return false;
      }
    }
    return null;
  }

  if (field.type === "integer") {
    if (typeof raw === "number" && Number.isFinite(raw)) {
      return String(Math.trunc(raw));
    }
    if (typeof raw === "string") {
      const trimmed = raw.trim();
      if (!trimmed) {
        return "";
      }
      const parsed = Number(trimmed);
      if (!Number.isInteger(parsed)) {
        throw new Error(`invalid integer field: ${field.key}`);
      }
      return String(parsed);
    }
    return "";
  }

  if (typeof raw === "string") {
    return raw;
  }
  if (raw === null || raw === undefined) {
    return "";
  }
  return String(raw);
}

function parseJsonObjects(rawText: string): Record<string, unknown>[] {
  const trimmed = rawText.trim();
  if (!trimmed) {
    return [];
  }

  try {
    const parsed = JSON.parse(trimmed) as unknown;
    if (Array.isArray(parsed)) {
      const rows = parsed.filter(isObject);
      if (rows.length !== parsed.length) {
        throw new Error("bulk JSON array must contain objects only");
      }
      return rows;
    }
    if (isObject(parsed)) {
      return [parsed];
    }
    throw new Error("bulk JSON must be an object or array of objects");
  } catch {
    const rows: Record<string, unknown>[] = [];
    const lines = rawText
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean);

    for (const [index, line] of lines.entries()) {
      let parsed: unknown;
      try {
        parsed = JSON.parse(line);
      } catch (error) {
        throw new Error(`invalid JSON at line ${index + 1}: ${String(error)}`);
      }
      if (!isObject(parsed)) {
        throw new Error(`line ${index + 1} must be a JSON object`);
      }
      rows.push(parsed);
    }
    return rows;
  }
}

function looksLikeJsonBulkInput(rawText: string): boolean {
  const trimmed = rawText.trim();
  if (!trimmed) {
    return false;
  }
  return trimmed.startsWith("{") || trimmed.startsWith("[");
}

export function availableBulkModes(
  channel: string,
  schema: ChannelCredentialSchema,
  supportsOAuth: boolean
): CredentialBulkMode[] {
  if (channel === "claudecode") {
    return ["json", "claudecode_cookie"];
  }
  if (isKeyOnlySchema(schema)) {
    return ["keys"];
  }
  if (supportsOAuth) {
    return ["json"];
  }
  return ["json"];
}

export function defaultBulkMode(
  channel: string,
  schema: ChannelCredentialSchema,
  supportsOAuth: boolean
): CredentialBulkMode {
  return availableBulkModes(channel, schema, supportsOAuth)[0] ?? "json";
}

function parseLineValueImports(
  channel: string,
  keyField: string,
  rawText: string
): BulkCredentialImportEntry[] {
  const lines = rawText
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);

  return lines.map((line) => {
    const base = createEmptyCredentialFormState(channel);
    return {
      name: null,
      enabled: true,
      secretValues: {
        ...base.secretValues,
        [keyField]: line
      }
    };
  });
}

function applyPlainSecretValues(
  schema: ChannelCredentialSchema,
  source: Record<string, unknown>,
  defaults: Record<string, CredentialFieldValue>
): Record<string, CredentialFieldValue> {
  const result = { ...defaults };
  for (const field of schema.fields) {
    if (!(field.key in source)) {
      continue;
    }
    result[field.key] = coerceFieldValue(field, source[field.key]);
  }
  return result;
}

function parseOptionalInteger(value: unknown, field: string): number | undefined {
  if (value === null || value === undefined) {
    return undefined;
  }
  if (typeof value === "number" && Number.isInteger(value)) {
    return value;
  }
  if (typeof value === "string" && value.trim()) {
    const parsed = Number(value.trim());
    if (Number.isInteger(parsed)) {
      return parsed;
    }
  }
  throw new Error(`${field} must be an integer`);
}

function entryFromJsonObject(
  channel: string,
  schema: ChannelCredentialSchema,
  row: Record<string, unknown>
): BulkCredentialImportEntry {
  const base = createEmptyCredentialFormState(channel);
  let secretValues = { ...base.secretValues };

  if (isObject(row.secretValues)) {
    secretValues = applyPlainSecretValues(schema, row.secretValues, secretValues);
  } else if (isObject(row.secret_json)) {
    secretValues = secretValuesFromSecretJson(channel, row.secret_json);
  } else if ("Builtin" in row || "Custom" in row) {
    secretValues = secretValuesFromSecretJson(channel, row);
  } else {
    secretValues = applyPlainSecretValues(schema, row, secretValues);
  }

  const id = parseOptionalInteger(row.id, "id");
  const enabled = typeof row.enabled === "boolean" ? row.enabled : true;
  const name = typeof row.name === "string" ? row.name : null;
  const settingsPayload = isObject(row.settings_json) ? row.settings_json : null;

  return {
    id,
    name,
    enabled,
    settingsPayload,
    secretValues
  };
}

export function parseBulkCredentialText(args: {
  channel: string;
  schema: ChannelCredentialSchema;
  mode: CredentialBulkMode;
  rawText: string;
}): BulkCredentialImportEntry[] {
  const { channel, schema, mode, rawText } = args;
  const trimmed = rawText.trim();
  if (!trimmed) {
    return [];
  }

  if (mode === "keys") {
    if (looksLikeJsonBulkInput(rawText)) {
      const rows = parseJsonObjects(rawText);
      return rows.map((row) => entryFromJsonObject(channel, schema, row));
    }
    const keyField = fieldByKey(schema, "api_key") ?? fieldByKey(schema, "cookie");
    if (!keyField) {
      throw new Error("this channel does not support key-line bulk import");
    }
    return parseLineValueImports(channel, keyField.key, rawText);
  }

  if (mode === "claudecode_cookie") {
    if (looksLikeJsonBulkInput(rawText)) {
      const rows = parseJsonObjects(rawText);
      return rows.map((row) => entryFromJsonObject(channel, schema, row));
    }
    const cookieField = fieldByKey(schema, "cookie");
    if (!cookieField) {
      throw new Error("this channel does not support cookie-line bulk import");
    }
    return parseLineValueImports(channel, cookieField.key, rawText);
  }

  const rows = parseJsonObjects(rawText);
  return rows.map((row) => entryFromJsonObject(channel, schema, row));
}

export function buildBulkExportText(args: {
  channel: string;
  schema: ChannelCredentialSchema;
  mode: CredentialBulkMode;
  credentialRows: CredentialQueryRow[];
}): string {
  const { channel, mode, credentialRows } = args;
  if (credentialRows.length === 0) {
    return "";
  }

  if (mode === "keys" || mode === "claudecode_cookie") {
    return credentialRows
      .map((row) => {
        const secretValues = secretValuesFromSecretJson(channel, row.secret_json);
        const value =
          mode === "claudecode_cookie"
            ? secretValues.cookie
            : (secretValues.api_key ?? secretValues.cookie);
        return typeof value === "string" ? value.trim() : "";
      })
      .filter(Boolean)
      .join("\n");
  }

  return credentialRows
    .map((row) =>
      JSON.stringify({
        id: row.id,
        name: row.name,
        enabled: row.enabled,
        settings_json: row.settings_json,
        secret_json: row.secret_json
      })
    )
    .join("\n");
}

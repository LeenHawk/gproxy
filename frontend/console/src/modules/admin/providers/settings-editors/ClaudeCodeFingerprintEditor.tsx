import { useState } from "react";

import { Button, Input, Label, TextArea } from "../../../../components/ui";
import {
  CLAUDECODE_FINGERPRINT_FIELDS,
  DEFAULT_CLAUDECODE_FINGERPRINT,
  DEFAULT_CLAUDECODE_FINGERPRINT_OBJECT,
  type ClaudeCodeFingerprint,
  type ClaudeCodeFingerprintKey,
} from "../channel-constants";
import { CollapsibleSection, type TranslateFn } from "./CollapsibleSection";

type ParseResult =
  | { ok: true; value: ClaudeCodeFingerprint }
  | { ok: false; message: string };

function parseFingerprint(value: string): ParseResult {
  const trimmed = value.trim();
  if (trimmed === "") {
    return { ok: true, value: {} };
  }
  try {
    const parsed: unknown = JSON.parse(trimmed);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return { ok: false, message: "Expected a JSON object." };
    }
    return { ok: true, value: parsed as ClaudeCodeFingerprint };
  } catch (error) {
    return { ok: false, message: error instanceof Error ? error.message : String(error) };
  }
}

function fieldValue(fingerprint: ClaudeCodeFingerprint, key: ClaudeCodeFingerprintKey): string {
  const value = fingerprint[key];
  return typeof value === "string" ? value : "";
}

function summarize(fingerprint: ClaudeCodeFingerprint, t: TranslateFn) {
  const cliVersion = fieldValue(fingerprint, "cli_version");
  const sdkVersion = fieldValue(fingerprint, "stainless_package_version");
  if (!cliVersion && !sdkVersion) {
    return t("common.none");
  }
  return t("providers.fingerprint.summary", {
    cli: cliVersion || DEFAULT_CLAUDECODE_FINGERPRINT_OBJECT.cli_version,
    sdk: sdkVersion || DEFAULT_CLAUDECODE_FINGERPRINT_OBJECT.stainless_package_version,
  });
}

export function ClaudeCodeFingerprintEditor({
  value,
  onChange,
  t,
}: {
  value: string;
  onChange: (value: string) => void;
  t: TranslateFn;
}) {
  const [expanded, setExpanded] = useState(false);
  const parsed = parseFingerprint(value);

  const commitField = (key: ClaudeCodeFingerprintKey, nextValue: string) => {
    const current = parsed.ok ? parsed.value : {};
    const next: ClaudeCodeFingerprint = { ...current };
    if (nextValue.trim() === "") {
      delete next[key];
    } else {
      next[key] = nextValue;
    }
    onChange(JSON.stringify(next, null, 2));
  };

  return (
    <CollapsibleSection
      title={t("providers.fingerprint.title")}
      summary={parsed.ok ? summarize(parsed.value, t) : t("providers.fingerprint.invalid")}
      expanded={expanded}
      onToggle={() => setExpanded((v) => !v)}
      expandLabel={t("common.show")}
      collapseLabel={t("providers.routing.collapse")}
      actions={
        <Button variant="neutral" onClick={() => onChange(DEFAULT_CLAUDECODE_FINGERPRINT)}>
          {t("providers.fingerprint.resetDefault")}
        </Button>
      }
    >
      <p className="text-xs text-muted">{t("providers.fingerprint.hint")}</p>
      {parsed.ok ? (
        <div className="grid gap-3 md:grid-cols-2">
          {CLAUDECODE_FINGERPRINT_FIELDS.map((key) => (
            <div key={key}>
              <Label>{t(`providers.fingerprint.field.${key}`)}</Label>
              <Input
                value={fieldValue(parsed.value, key)}
                onChange={(next) => commitField(key, next)}
                placeholder={DEFAULT_CLAUDECODE_FINGERPRINT_OBJECT[key]}
              />
            </div>
          ))}
        </div>
      ) : (
        <div className="space-y-2">
          <TextArea value={value} onChange={onChange} rows={8} />
          <p className="text-xs text-danger">{parsed.message}</p>
        </div>
      )}
    </CollapsibleSection>
  );
}

import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { PencilIcon } from "lucide-react";
import { tlsPresetsQuery } from "@/api/providers";
import { JsonField, parseJsonText } from "@/components/form/json-field";
import { Button } from "@/components/ui/button";
import {
  Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import {
  Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from "@/components/ui/select";

const SENTINEL_DEFAULT = "__default__";
const SENTINEL_CUSTOM = "__custom__";

/** Stable stringify for deep-equality comparison of fingerprint blobs. */
function stableStringify(v: unknown): string {
  if (v === null || v === undefined) return String(v);
  if (typeof v !== "object" || Array.isArray(v)) return JSON.stringify(v);
  const obj = v as Record<string, unknown>;
  const sorted: Record<string, unknown> = {};
  for (const k of Object.keys(obj).sort()) {
    sorted[k] = obj[k];
  }
  return JSON.stringify(sorted, (_, val) =>
    val !== null && typeof val === "object" && !Array.isArray(val)
      ? Object.fromEntries(Object.entries(val as Record<string, unknown>).sort(([a], [b]) => a.localeCompare(b)))
      : val
  );
}

function deepEqual(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (a === null || a === undefined || b === null || b === undefined) return false;
  return stableStringify(a) === stableStringify(b);
}

/** Pretty JSON for editing; null/undefined → empty string. */
function toDraft(v: unknown): string {
  return v == null ? "" : JSON.stringify(v, null, 2);
}

interface TlsFingerprintFieldProps {
  value: unknown;
  onChange: (v: unknown) => void;
  label?: string;
}

export function TlsFingerprintField({ value, onChange, label }: TlsFingerprintFieldProps) {
  const { t } = useTranslation("providers");
  const { t: tc } = useTranslation("common");
  const { data: presets = [] } = useQuery(tlsPresetsQuery);

  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState("");
  const [error, setError] = useState(false);

  // Seed the editor from the current value whenever the dialog opens.
  useEffect(() => {
    if (open) {
      setDraft(toDraft(value));
      setError(false);
    }
  }, [open, value]);

  // What the trigger button shows (reflects the saved value, not the draft).
  const savedMatch = value == null ? null : presets.find((p) => deepEqual(value, p.fingerprint)) ?? null;
  const triggerLabel = value == null
    ? t("tls.default")
    : savedMatch
    ? savedMatch.label
    : t("tls.customExisting");

  // The Select's shown value reflects the *draft* (recomputed on every keystroke).
  const trimmed = draft.trim();
  const draftParsed = trimmed === "" ? null : parseJsonText(draft);
  const draftMatch = draftParsed && draftParsed.ok
    ? presets.find((p) => deepEqual(draftParsed.value, p.fingerprint)) ?? null
    : null;
  const selectValue = trimmed === ""
    ? SENTINEL_DEFAULT
    : draftMatch
    ? draftMatch.id
    : SENTINEL_CUSTOM;
  const showCustomOption = selectValue === SENTINEL_CUSTOM;

  function handlePreset(val: string) {
    setError(false);
    if (val === SENTINEL_CUSTOM) return; // not a fill action, just reflects state
    if (val === SENTINEL_DEFAULT) {
      setDraft("");
    } else {
      const preset = presets.find((p) => p.id === val);
      if (preset) setDraft(JSON.stringify(preset.fingerprint, null, 2));
    }
  }

  function handleApply() {
    if (trimmed === "") {
      onChange(null);
      setOpen(false);
      return;
    }
    const parsed = parseJsonText(draft);
    if (!parsed.ok) {
      setError(true);
      return;
    }
    onChange(parsed.value);
    setOpen(false);
  }

  return (
    <div className="grid gap-2">
      {label && <Label>{label}</Label>}
      <Button
        type="button"
        variant="outline"
        className="w-fit justify-between gap-2"
        aria-label={label ? `${label}: ${triggerLabel}` : triggerLabel}
        onClick={() => setOpen(true)}
      >
        <span>{triggerLabel}</span>
        <PencilIcon className="size-3.5 text-muted-foreground" />
      </Button>

      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="sm:max-w-lg max-h-[85vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle>{t("tls.title")}</DialogTitle>
          </DialogHeader>

          <div className="grid gap-4">
            <div className="grid gap-1.5">
              <Label htmlFor="tls-preset">{t("tls.preset")}</Label>
              <Select value={selectValue} onValueChange={handlePreset}>
                <SelectTrigger id="tls-preset" className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={SENTINEL_DEFAULT}>{t("tls.default")}</SelectItem>
                  {presets.map((preset) => (
                    <SelectItem key={preset.id} value={preset.id}>
                      {preset.label}
                    </SelectItem>
                  ))}
                  {showCustomOption && (
                    <SelectItem value={SENTINEL_CUSTOM} disabled>
                      {t("tls.custom")}
                    </SelectItem>
                  )}
                </SelectContent>
              </Select>
            </div>

            <div className="grid gap-1.5">
              <Label htmlFor="tls-json">JSON</Label>
              <JsonField
                id="tls-json"
                value={draft}
                onChange={(text) => { setDraft(text); setError(false); }}
                rows={10}
                hint={t("tls.jsonHint")}
              />
              {error && <p className="text-xs text-destructive">{t("tls.invalid")}</p>}
            </div>
          </div>

          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => setOpen(false)}>
              {tc("actions.cancel")}
            </Button>
            <Button type="button" onClick={handleApply}>
              {t("tls.apply")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

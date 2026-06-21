import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { upsertCredential } from "@/api/credentials";
import { ApiError } from "@/api/http";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";

interface ParsedLine { key: string; label: string | null }
interface LineResult { prefix: string; ok: boolean; error?: string }
type Phase = "idle" | "importing" | "done";

function intOrNull(v: string): number | null {
  const n = Number(v);
  return v.trim() !== "" && Number.isInteger(n) && n > 0 ? n : null;
}

function parseLines(text: string): { lines: ParsedLine[]; dupes: number } {
  const seen = new Set<string>();
  const lines: ParsedLine[] = [];
  let dupes = 0;
  for (const raw of text.split("\n")) {
    const t = raw.trim();
    if (!t || t.startsWith("#")) continue;
    const sep = t.search(/[,\t]/);
    const key = sep === -1 ? t : t.slice(0, sep).trim();
    const label = sep === -1 ? null : t.slice(sep + 1).trim() || null;
    if (!key) continue;
    if (seen.has(key)) { dupes++; continue; }
    seen.add(key);
    lines.push({ key, label });
  }
  return { lines, dupes };
}

export interface CredentialBulkImportProps {
  providerId: number;
  onClose: () => void;
}

export function CredentialBulkImport({ providerId, onClose }: CredentialBulkImportProps) {
  const { t } = useTranslation("providers");
  const queryClient = useQueryClient();

  const [text, setText] = useState("");
  const [weight, setWeight] = useState("100");
  const [rpm, setRpm] = useState("");
  const [tpm, setTpm] = useState("");
  const [proxyUrl, setProxyUrl] = useState("");
  const [enabled, setEnabled] = useState(true);

  const [phase, setPhase] = useState<Phase>("idle");
  const [done, setDone] = useState(0);
  const [total, setTotal] = useState(0);
  const [results, setResults] = useState<LineResult[]>([]);
  const [dupesSkipped, setDupesSkipped] = useState(0);

  const created = results.filter((r) => r.ok).length;
  const failed = results.filter((r) => !r.ok).length;
  const { lines: preview } = parseLines(text);
  const isEmpty = preview.length === 0;

  async function runImport() {
    const { lines, dupes } = parseLines(text);
    if (lines.length === 0) return;

    setPhase("importing");
    setDone(0);
    setTotal(lines.length);
    setResults([]);
    setDupesSkipped(dupes);

    const w = intOrNull(weight) ?? 100;
    const accum: LineResult[] = [];

    for (let i = 0; i < lines.length; i++) {
      const { key, label } = lines[i];
      const prefix = key.slice(0, 8) + "…";
      try {
        await upsertCredential(providerId, {
          id: null, label, kind: "api_key",
          secret_json: { api_key: key },
          weight: w,
          rpm_limit: intOrNull(rpm),
          tpm_limit: intOrNull(tpm),
          proxy_url: proxyUrl.trim() || null,
          enabled,
        });
        accum.push({ prefix, ok: true });
        void queryClient.invalidateQueries({ queryKey: ["providers", providerId, "credentials"] });
      } catch (err) {
        accum.push({ prefix, ok: false, error: err instanceof ApiError ? err.message : String(err) });
      }
      setDone(i + 1);
      setResults([...accum]);
    }

    setPhase("done");
    setText(""); // keys are transient — clear after import
  }

  const busy = phase === "importing";

  return (
    <div className="grid gap-4">
      <div className="grid gap-2">
        <Label htmlFor="bulk-text">{t("creds.bulk.textareaLabel")}</Label>
        <Textarea
          id="bulk-text"
          placeholder={t("creds.bulk.textareaHint")}
          rows={8}
          value={text}
          onChange={(e) => setText(e.target.value)}
          disabled={busy}
          className="font-mono text-xs"
        />
      </div>

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
        <div className="grid gap-2">
          <Label htmlFor="b-weight">{t("fields.weight")}</Label>
          <Input id="b-weight" inputMode="numeric" value={weight}
            onChange={(e) => setWeight(e.target.value)} disabled={busy} />
        </div>
        <div className="grid gap-2">
          <Label htmlFor="b-rpm">{t("fields.rpm")}</Label>
          <Input id="b-rpm" inputMode="numeric" value={rpm}
            onChange={(e) => setRpm(e.target.value)} disabled={busy} />
        </div>
        <div className="grid gap-2">
          <Label htmlFor="b-tpm">{t("fields.tpm")}</Label>
          <Input id="b-tpm" inputMode="numeric" value={tpm}
            onChange={(e) => setTpm(e.target.value)} disabled={busy} />
        </div>
      </div>

      <div className="grid gap-2">
        <Label htmlFor="b-proxy">{t("fields.proxyUrl")}</Label>
        <Input id="b-proxy" value={proxyUrl}
          onChange={(e) => setProxyUrl(e.target.value)} disabled={busy} />
      </div>

      <div className="flex items-center justify-between">
        <Label htmlFor="b-enabled">{t("fields.enabled")}</Label>
        <Switch id="b-enabled" checked={enabled} onCheckedChange={setEnabled} disabled={busy} />
      </div>

      {busy && (
        <p className="text-sm text-muted-foreground" role="status" aria-live="polite">
          {t("creds.bulk.importing", { done, total })}
        </p>
      )}

      {phase === "done" && (
        <div role="status" aria-live="assertive" className="grid gap-2">
          <p className="text-sm font-medium">
            {t("creds.bulk.summary", { created, failed, dupes: dupesSkipped })}
          </p>
          {results.filter((r) => !r.ok).map((r, i) => (
            <p key={i} className="font-mono text-xs text-destructive">{r.prefix}: {r.error}</p>
          ))}
        </div>
      )}

      <div className="flex justify-end gap-2">
        {phase === "done" ? (
          <Button onClick={onClose}>{t("creds.bulk.close")}</Button>
        ) : (
          <>
            <Button variant="outline" onClick={onClose} disabled={busy}>{t("creds.bulk.close")}</Button>
            <Button disabled={busy || isEmpty} onClick={() => { void runImport(); }}>
              {busy
                ? t("creds.bulk.importing", { done, total })
                : isEmpty
                  ? t("creds.bulk.empty")
                  : t("creds.bulk.import", { count: preview.length })}
            </Button>
          </>
        )}
      </div>
    </div>
  );
}

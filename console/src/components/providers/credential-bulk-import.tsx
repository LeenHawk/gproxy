import { useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Upload, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { importCredentials } from "@/api/credentials";
import { ApiError } from "@/api/http";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import type { ChannelMeta } from "@/lib/channel-meta";
import {
  bulkModeFor, dedupeItems, parseJsonInput, parseTokens,
  type ParseOutcome,
} from "@/lib/credential-bulk-parse";

interface LineResult { display: string; ok: boolean; error?: string }
interface FileEntry { name: string; outcome: ParseOutcome }
type Phase = "idle" | "importing" | "done";

function intOrNull(v: string): number | null {
  const n = Number(v);
  return v.trim() !== "" && Number.isInteger(n) && n > 0 ? n : null;
}

export interface CredentialBulkImportProps {
  providerId: number;
  meta: ChannelMeta;
  metadataAuthoritative: boolean;
  onClose: () => void;
}

export function CredentialBulkImport({
  providerId, meta, metadataAuthoritative, onClose,
}: CredentialBulkImportProps) {
  const { t } = useTranslation("providers");
  const queryClient = useQueryClient();
  const mode = bulkModeFor(meta.family);
  const fileInput = useRef<HTMLInputElement>(null);

  const [text, setText] = useState("");
  const [files, setFiles] = useState<FileEntry[]>([]);
  const [weight, setWeight] = useState("100");
  const [rpm, setRpm] = useState("");
  const [tpm, setTpm] = useState("");
  const [proxyUrl, setProxyUrl] = useState("");
  const [enabled, setEnabled] = useState(true);

  const [phase, setPhase] = useState<Phase>("idle");
  const [total, setTotal] = useState(0);
  const [results, setResults] = useState<LineResult[]>([]);
  const [dupesSkipped, setDupesSkipped] = useState(0);
  const [existing, setExisting] = useState(0);

  const created = results.filter((r) => r.ok).length - existing;
  const failed = results.filter((r) => !r.ok).length;

  const textOutcome = mode === "tokens"
    ? parseTokens(meta.family, text)
    : parseJsonInput(meta.family, text, "input");
  const outcomes = [textOutcome, ...files.map((f) => f.outcome)];
  const parseErrors = outcomes.flatMap((o) => o.errors);
  const { items, dupes } = dedupeItems(outcomes.flatMap((o) => o.items));
  const isEmpty = items.length === 0;

  async function addFiles(list: FileList | null) {
    if (!list || list.length === 0) return;
    const read = await Promise.all(Array.from(list).map(async (file) => ({
      name: file.name,
      outcome: parseJsonInput(meta.family, await file.text(), file.name),
    })));
    setFiles((prev) => [...prev, ...read]);
    if (fileInput.current) fileInput.current.value = ""; // allow re-picking the same file
  }

  async function runImport() {
    if (!metadataAuthoritative || items.length === 0) return;

    setPhase("importing");
    setTotal(items.length);
    setResults([]);
    setExisting(0);
    setDupesSkipped(dupes);

    const w = intOrNull(weight) ?? 100;
    try {
      // One request; the server creates/dedupes each item and reports per-item results.
      const outcome = await importCredentials(providerId, items.map(({ label, secret }) => ({
        id: null, label, kind: meta.family,
        secret_json: secret,
        weight: w,
        rpm_limit: intOrNull(rpm),
        tpm_limit: intOrNull(tpm),
        proxy_url: proxyUrl.trim() || null,
        enabled,
      })));
      setExisting(outcome.existing);
      setResults(outcome.results.map((r) => ({
        display: items[r.index]?.display ?? `#${r.index}`,
        ok: r.status !== "error",
        error: r.error,
      })));
      void queryClient.invalidateQueries({ queryKey: ["providers", providerId, "credentials"] });
      setText(""); // secrets are transient — clear after import
      setFiles([]);
    } catch (err) {
      // Whole-request failure (auth/network/validation): every item is unsent.
      const message = err instanceof ApiError ? err.message : String(err);
      setResults(items.map(({ display }) => ({ display, ok: false, error: message })));
    }

    setPhase("done");
  }

  const busy = phase === "importing";

  return (
    <div className="grid gap-4">
      <div className="grid gap-2">
        <Label htmlFor="bulk-text">
          {mode === "tokens" ? t("creds.bulk.textareaLabel") : t("creds.bulk.jsonLabel")}
        </Label>
        <Textarea
          id="bulk-text"
          placeholder={mode === "tokens" ? t("creds.bulk.textareaHint") : t("creds.bulk.jsonHint")}
          rows={8}
          value={text}
          onChange={(e) => setText(e.target.value)}
          disabled={busy}
          className="font-mono text-xs"
        />
      </div>

      {mode === "json" && (
        <div className="grid gap-2">
          <input
            ref={fileInput}
            type="file"
            accept=".json,application/json"
            multiple
            className="hidden"
            onChange={(e) => { void addFiles(e.target.files); }}
          />
          <div>
            <Button variant="outline" size="sm" disabled={busy} onClick={() => fileInput.current?.click()}>
              <Upload className="size-4" aria-hidden />
              {t("creds.bulk.upload")}
            </Button>
          </div>
          {files.map((f, i) => (
            <div key={i} className="flex items-center justify-between rounded-md border px-2 py-1 text-xs">
              <span className="truncate font-mono">
                {f.name} — {t("creds.bulk.fileItems", { count: f.outcome.items.length })}
              </span>
              <Button
                variant="ghost" size="icon" className="size-6" disabled={busy}
                aria-label={t("creds.bulk.removeFile")}
                onClick={() => setFiles((prev) => prev.filter((_, j) => j !== i))}
              >
                <X className="size-3" aria-hidden />
              </Button>
            </div>
          ))}
        </div>
      )}

      {phase === "idle" && parseErrors.length > 0 && (
        <div className="grid gap-1">
          {parseErrors.slice(0, 5).map((e, i) => (
            <p key={i} className="font-mono text-xs text-destructive">
              {t(`creds.bulk.err.${e.code}`, { source: e.source })}
            </p>
          ))}
          {parseErrors.length > 5 && (
            <p className="text-xs text-muted-foreground">
              {t("creds.bulk.errMore", { count: parseErrors.length - 5 })}
            </p>
          )}
        </div>
      )}

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
          {t("creds.bulk.importing", { total })}
        </p>
      )}

      {phase === "done" && (
        <div role="status" aria-live="assertive" className="grid gap-2">
          <p className="text-sm font-medium">
            {t("creds.bulk.summary", { created, existing, failed, dupes: dupesSkipped })}
          </p>
          {results.filter((r) => !r.ok).map((r, i) => (
            <p key={i} className="font-mono text-xs text-destructive">{r.display}: {r.error}</p>
          ))}
        </div>
      )}

      <div className="flex justify-end gap-2">
        {phase === "done" ? (
          <Button onClick={onClose}>{t("creds.bulk.close")}</Button>
        ) : (
          <>
            <Button variant="outline" onClick={onClose} disabled={busy}>{t("creds.bulk.close")}</Button>
            <Button
              disabled={busy || isEmpty || !metadataAuthoritative}
              onClick={() => { void runImport(); }}
            >
              {busy
                ? t("creds.bulk.importing", { total })
                : isEmpty
                  ? t("creds.bulk.empty")
                  : t("creds.bulk.import", { count: items.length })}
            </Button>
          </>
        )}
      </div>
    </div>
  );
}

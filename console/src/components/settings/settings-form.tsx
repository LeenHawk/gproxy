import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  type InstanceSettings,
  type InstanceSettingsInput,
  upsertInstanceSettings,
} from "@/api/settings";
import { ApiError } from "@/api/http";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";

// ---- helpers ----------------------------------------------------------------

function SwitchField({ id, label, checked, onCheckedChange }: { id: string; label: string; checked: boolean; onCheckedChange: (v: boolean) => void }) {
  return (
    <div className="flex items-center justify-between gap-4 py-1">
      <Label htmlFor={id} className="cursor-pointer">{label}</Label>
      <Switch id={id} checked={checked} onCheckedChange={onCheckedChange} />
    </div>
  );
}

function Section({ title, children, span2 }: { title: string; children: React.ReactNode; span2?: boolean }) {
  return (
    <Card className={span2 ? "md:col-span-2" : undefined}>
      <CardHeader><CardTitle className="text-base">{title}</CardTitle></CardHeader>
      <CardContent className="grid gap-2">{children}</CardContent>
    </Card>
  );
}

// ---- form state -------------------------------------------------------------

interface FormState {
  instanceName: string; proxy: string; spoofEmulation: string;
  enableUsage: boolean; enableUpstreamLog: boolean; enableUpstreamLogBody: boolean;
  enableDownstreamLog: boolean; enableDownstreamLogBody: boolean;
  disableLogRedaction: boolean; enableTokenizerDownload: boolean;
  updateChannel: string; retentionDays: string;
}

function initState(s?: InstanceSettings): FormState {
  const spoof = s?.spoof_emulation === true ? "on" : s?.spoof_emulation === false ? "off" : "inherit";
  return {
    instanceName: s?.instance_name ?? "", proxy: s?.proxy ?? "", spoofEmulation: spoof,
    enableUsage: s?.enable_usage ?? true, enableUpstreamLog: s?.enable_upstream_log ?? false,
    enableUpstreamLogBody: s?.enable_upstream_log_body ?? false,
    enableDownstreamLog: s?.enable_downstream_log ?? false,
    enableDownstreamLogBody: s?.enable_downstream_log_body ?? false,
    disableLogRedaction: s?.disable_log_redaction ?? false,
    enableTokenizerDownload: s?.enable_tokenizer_download ?? false,
    updateChannel: s?.update_channel ?? "default",
    retentionDays: s?.retention_days != null ? String(s.retention_days) : "",
  };
}

// ---- main component ---------------------------------------------------------

export function SettingsForm({ settings, onSaved }: { settings?: InstanceSettings; onSaved?: (s: InstanceSettings) => void }) {
  const { t } = useTranslation("settings");
  const qc = useQueryClient();
  const isEdit = settings?.id != null;

  const [f, setF] = useState<FormState>(() => initState(settings));
  const set = <K extends keyof FormState>(k: K) => (v: FormState[K]) => setF((prev) => ({ ...prev, [k]: v }));

  const [formError, setFormError] = useState<string | null>(null);
  const nameRequired = t("form.nameRequired");
  const nameHasError = formError === nameRequired;
  const showSensitiveWarning = f.disableLogRedaction || f.enableUpstreamLogBody || f.enableDownstreamLogBody;

  const save = useMutation({
    mutationFn: async () => {
      if (!f.instanceName.trim()) throw new ApiError(0, "bad_request", nameRequired);
      const retDays = f.retentionDays.trim() === "" || Number(f.retentionDays) <= 0 ? null : Number(f.retentionDays);
      const input: InstanceSettingsInput = {
        id: settings?.id ?? null,
        instance_name: f.instanceName.trim(),
        proxy: f.proxy.trim() || null,
        spoof_emulation: f.spoofEmulation === "on" ? true : f.spoofEmulation === "off" ? false : null,
        enable_usage: f.enableUsage,
        enable_upstream_log: f.enableUpstreamLog,
        enable_upstream_log_body: f.enableUpstreamLogBody,
        enable_downstream_log: f.enableDownstreamLog,
        enable_downstream_log_body: f.enableDownstreamLogBody,
        disable_log_redaction: f.disableLogRedaction,
        enable_tokenizer_download: f.enableTokenizerDownload,
        update_channel: f.updateChannel === "default" ? null : f.updateChannel,
        retention_days: retDays,
      };
      return upsertInstanceSettings(input);
    },
    onSuccess: (result) => { void qc.invalidateQueries({ queryKey: ["instance-settings"] }); toast.success(t("saved")); setFormError(null); onSaved?.(result); },
    onError: (e) => { setFormError(e instanceof ApiError ? e.message : String(e)); },
  });

  return (
    <form onSubmit={(e) => { e.preventDefault(); save.mutate(); }} className="grid gap-4">
      {showSensitiveWarning && (
        <div role="alert" className="rounded-md border border-amber-500 bg-amber-50 p-3 text-sm text-amber-900 dark:bg-amber-950 dark:text-amber-200">
          {t("warnings.sensitive")}
        </div>
      )}
      {formError && (
        <div role="alert" className="rounded-md border border-destructive bg-destructive/10 p-3 text-sm text-destructive" id="settings-form-err">
          {formError}
        </div>
      )}

      <div className="grid gap-4 md:grid-cols-2">
        <Section title={t("sections.identity")}>
          <div className="grid gap-1">
            <Label htmlFor="settings-name">{t("fields.instanceName")}</Label>
            <Input id="settings-name" value={f.instanceName} onChange={(e) => set("instanceName")(e.target.value)}
              readOnly={isEdit} className={isEdit ? "bg-muted" : undefined}
              aria-invalid={nameHasError ? true : undefined} aria-describedby={nameHasError ? "settings-name-err" : undefined} />
            {nameHasError && <p id="settings-name-err" className="text-xs text-destructive">{formError}</p>}
          </div>
        </Section>

        <Section title={t("sections.outbound")}>
          <div className="grid gap-1">
            <Label htmlFor="settings-proxy">{t("fields.proxy")}</Label>
            <Input id="settings-proxy" value={f.proxy} onChange={(e) => set("proxy")(e.target.value)} placeholder="http://proxy:8080" />
          </div>
          <div className="grid gap-1">
            <Label htmlFor="settings-spoof">{t("fields.spoofEmulation")}</Label>
            <Select value={f.spoofEmulation} onValueChange={set("spoofEmulation")}>
              <SelectTrigger id="settings-spoof"><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="inherit">{t("spoof.inherit")}</SelectItem>
                <SelectItem value="on">{t("spoof.on")}</SelectItem>
                <SelectItem value="off">{t("spoof.off")}</SelectItem>
              </SelectContent>
            </Select>
          </div>
        </Section>

        <Section title={t("sections.usageLogging")} span2>
          <SwitchField id="s-usage" label={t("fields.enableUsage")} checked={f.enableUsage} onCheckedChange={set("enableUsage")} />
          <SwitchField id="s-uplog" label={t("fields.enableUpstreamLog")} checked={f.enableUpstreamLog} onCheckedChange={set("enableUpstreamLog")} />
          <SwitchField id="s-upbody" label={t("fields.enableUpstreamLogBody")} checked={f.enableUpstreamLogBody} onCheckedChange={set("enableUpstreamLogBody")} />
          <SwitchField id="s-dnlog" label={t("fields.enableDownstreamLog")} checked={f.enableDownstreamLog} onCheckedChange={set("enableDownstreamLog")} />
          <SwitchField id="s-dnbody" label={t("fields.enableDownstreamLogBody")} checked={f.enableDownstreamLogBody} onCheckedChange={set("enableDownstreamLogBody")} />
          <SwitchField id="s-redact" label={t("fields.disableLogRedaction")} checked={f.disableLogRedaction} onCheckedChange={set("disableLogRedaction")} />
        </Section>

        <Section title={t("sections.retention")}>
          <Label htmlFor="settings-retention">{t("fields.retentionDays")}</Label>
          <Input id="settings-retention" type="number" min="0" value={f.retentionDays} onChange={(e) => set("retentionDays")(e.target.value)} />
          {(!f.retentionDays.trim() || Number(f.retentionDays) <= 0) && (
            <p className="text-xs text-muted-foreground">{t("retention.forever")}</p>
          )}
        </Section>

        <Section title={t("sections.tokenizer")}>
          <SwitchField id="s-tokenizer" label={t("fields.enableTokenizerDownload")} checked={f.enableTokenizerDownload} onCheckedChange={set("enableTokenizerDownload")} />
        </Section>
      </div>

      <div>
        <Button type="submit" disabled={save.isPending}>
          {save.isPending ? "…" : t("save")}
        </Button>
      </div>
    </form>
  );
}

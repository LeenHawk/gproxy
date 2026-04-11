import { useEffect, useMemo, useState } from "react";

import { useI18n } from "../../app/i18n";
import { Button, Card, Input, Label, Select, StatusToggle } from "../../components/ui";
import { apiJson } from "../../lib/api";
import { authHeaders } from "../../lib/auth";
import type { GlobalSettings, UpdatePerformResponse } from "../../lib/types/admin";
import {
  normalizeUpdateChannel,
  normalizeUpdateSourceMode,
  resolveUpdateSourceValue,
  resolveUpdateTag,
  type UpdateChannel,
  type UpdateSourceMode,
} from "./global-settings";

const SPOOF_EMULATION_OPTIONS = [
  { value: "chrome_136", label: "Chrome 136" },
  { value: "chrome_135", label: "Chrome 135" },
  { value: "chrome_134", label: "Chrome 134" },
  { value: "chrome_133", label: "Chrome 133" },
  { value: "chrome_132", label: "Chrome 132" },
  { value: "chrome_131", label: "Chrome 131" },
  { value: "chrome_127", label: "Chrome 127" },
  { value: "safari_18", label: "Safari 18" },
  { value: "safari_18.2", label: "Safari 18.2" },
  { value: "safari_18.3", label: "Safari 18.3" },
  { value: "safari_18.5", label: "Safari 18.5" },
];

export function GlobalSettingsModule({
  sessionToken,
  notify,
}: {
  sessionToken: string;
  notify: (kind: "success" | "error" | "info", message: string) => void;
}) {
  const { t } = useI18n();
  const headers = useMemo(() => authHeaders(sessionToken), [sessionToken]);
  const [form, setForm] = useState<GlobalSettings | null>(null);
  const [updateSourceMode, setUpdateSourceMode] = useState<UpdateSourceMode>("github");
  const [customUpdateSource, setCustomUpdateSource] = useState("");
  const [updateChannel, setUpdateChannel] = useState<UpdateChannel>("release");
  const [refreshing, setRefreshing] = useState(false);
  const [updating, setUpdating] = useState(false);

  const load = async () => {
    setRefreshing(true);
    try {
      const next = await apiJson<GlobalSettings>("/admin/global-settings", {
        method: "GET",
        headers: authHeaders(sessionToken, false),
      });
      setForm(next);
      const nextMode = normalizeUpdateSourceMode(next.update_source);
      setUpdateSourceMode(nextMode);
      setCustomUpdateSource(nextMode === "custom" ? next.update_source : "");
    } finally {
      setRefreshing(false);
    }
  };

  useEffect(() => {
    void load().catch((error) => notify("error", error instanceof Error ? error.message : String(error)));
  }, [sessionToken]);

  const save = async () => {
    if (!form) return;
    try {
      await apiJson("/admin/global-settings/upsert", {
        method: "POST",
        headers,
        body: JSON.stringify({
          ...form,
          update_source: resolveUpdateSourceValue(updateSourceMode, customUpdateSource),
        }),
      });
      notify("success", t("globalSettings.saved"));
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  const performUpdate = async () => {
    try {
      setUpdating(true);
      const result = await apiJson<UpdatePerformResponse>("/admin/update", {
        method: "POST",
        headers,
        body: JSON.stringify({ tag: resolveUpdateTag(updateChannel) }),
      });
      notify("success", result.message || `${result.old_version} -> ${result.new_version}`);
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    } finally {
      setUpdating(false);
    }
  };

  if (!form) {
    return <Card title={t("globalSettings.title")}><p className="text-sm text-muted">{t("common.loading")}</p></Card>;
  }

  return (
    <Card
      title={t("globalSettings.title")}
      action={
        <div className="flex flex-wrap gap-2">
          <Button variant="neutral" onClick={() => void load()} disabled={refreshing}>
            {refreshing ? t("common.loading") : t("common.refresh")}
          </Button>
          <Button variant="danger" onClick={() => void performUpdate()} disabled={updating}>
            {updating ? t("common.loading") : t("common.update")}
          </Button>
          <Button onClick={() => void save()}>{t("common.save")}</Button>
        </div>
      }
    >
      <div className="grid gap-4 lg:grid-cols-2">
        <div>
          <Label>{t("globalSettings.field.host")}</Label>
          <Input
            value={form.host}
            onChange={(value) => setForm((current) => (current ? { ...current, host: value } : current))}
          />
        </div>
        <div>
          <Label>{t("globalSettings.field.port")}</Label>
          <Input
            value={String(form.port)}
            onChange={(value) =>
              setForm((current) => (current ? { ...current, port: Number(value) || 0 } : current))
            }
          />
        </div>
        <div>
          <Label>{t("globalSettings.field.proxy")}</Label>
          <Input
            value={form.proxy ?? ""}
            onChange={(value) => setForm((current) => (current ? { ...current, proxy: value } : current))}
            placeholder="http://127.0.0.1:7860"
          />
        </div>
        <div>
          <Label>{t("globalSettings.field.spoofEmulation")}</Label>
          <Select
            value={form.spoof_emulation}
            onChange={(value) => setForm((current) => (current ? { ...current, spoof_emulation: value } : current))}
            options={SPOOF_EMULATION_OPTIONS}
          />
        </div>
        <div>
          <Label>{t("globalSettings.field.updateSource")}</Label>
          <Select
            value={updateSourceMode}
            onChange={(value) => setUpdateSourceMode(value as UpdateSourceMode)}
            options={[
              { value: "github", label: t("globalSettings.updateSource.github") },
              { value: "web", label: t("globalSettings.updateSource.web") },
              { value: "custom", label: t("globalSettings.updateSource.custom") },
            ]}
          />
        </div>
        <div>
          <Label>{t("globalSettings.field.updateChannel")}</Label>
          <Select
            value={updateChannel}
            onChange={(value) => setUpdateChannel(normalizeUpdateChannel(value))}
            options={[
              { value: "release", label: t("globalSettings.updateChannel.release") },
              { value: "staging", label: t("globalSettings.updateChannel.staging") },
            ]}
          />
        </div>
        {updateSourceMode === "custom" ? (
          <div className="lg:col-span-2">
            <Label>{t("globalSettings.field.customUpdateSource")}</Label>
            <Input value={customUpdateSource} onChange={setCustomUpdateSource} />
          </div>
        ) : null}
        <div>
          <Label>{t("globalSettings.field.dsn")}</Label>
          <Input
            value={form.dsn}
            onChange={(value) => setForm((current) => (current ? { ...current, dsn: value } : current))}
          />
        </div>
        <div>
          <Label>{t("globalSettings.field.dataDir")}</Label>
          <Input
            value={form.data_dir}
            onChange={(value) => setForm((current) => (current ? { ...current, data_dir: value } : current))}
          />
        </div>
      </div>
      <div className="panel-shell mt-4 space-y-3">
        <div className="text-sm font-semibold text-text">{t("globalSettings.section.logging")}</div>
        <div className="grid gap-3 lg:grid-cols-2">
          <StatusToggle
            label={t("globalSettings.flag.enableUsage")}
            checked={form.enable_usage}
            onToggle={() =>
              setForm((current) => (current ? { ...current, enable_usage: !current.enable_usage } : current))
            }
            checkedLabel={t("common.enabled")}
            uncheckedLabel={t("common.disabled")}
          />
          <StatusToggle
            label={t("globalSettings.flag.enableUpstreamLog")}
            checked={form.enable_upstream_log}
            onToggle={() =>
              setForm((current) =>
                current ? { ...current, enable_upstream_log: !current.enable_upstream_log } : current,
              )
            }
            checkedLabel={t("common.enabled")}
            uncheckedLabel={t("common.disabled")}
          />
          <StatusToggle
            label={t("globalSettings.flag.enableUpstreamLogBody")}
            checked={form.enable_upstream_log_body}
            onToggle={() =>
              setForm((current) =>
                current ? { ...current, enable_upstream_log_body: !current.enable_upstream_log_body } : current,
              )
            }
            checkedLabel={t("common.enabled")}
            uncheckedLabel={t("common.disabled")}
          />
          <StatusToggle
            label={t("globalSettings.flag.enableDownstreamLog")}
            checked={form.enable_downstream_log}
            onToggle={() =>
              setForm((current) =>
                current ? { ...current, enable_downstream_log: !current.enable_downstream_log } : current,
              )
            }
            checkedLabel={t("common.enabled")}
            uncheckedLabel={t("common.disabled")}
          />
          <StatusToggle
            label={t("globalSettings.flag.enableDownstreamLogBody")}
            checked={form.enable_downstream_log_body}
            onToggle={() =>
              setForm((current) =>
                current
                  ? { ...current, enable_downstream_log_body: !current.enable_downstream_log_body }
                  : current,
              )
            }
            checkedLabel={t("common.enabled")}
            uncheckedLabel={t("common.disabled")}
          />
        </div>
      </div>
    </Card>
  );
}

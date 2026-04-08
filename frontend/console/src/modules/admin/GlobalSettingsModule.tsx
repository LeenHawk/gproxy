import { useEffect, useMemo, useState } from "react";

import { useI18n } from "../../app/i18n";
import { Button, Card, Input, Label } from "../../components/ui";
import { apiJson } from "../../lib/api";
import { authHeaders } from "../../lib/auth";
import type { GlobalSettings } from "../../lib/types/admin";

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

  useEffect(() => {
    void apiJson<GlobalSettings>("/admin/global-settings", {
      method: "GET",
      headers: authHeaders(sessionToken, false),
    }).then(setForm);
  }, [sessionToken]);

  const save = async () => {
    if (!form) return;
    try {
      await apiJson("/admin/global-settings/upsert", {
        method: "POST",
        headers,
        body: JSON.stringify(form),
      });
      notify("success", t("globalSettings.saved"));
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  if (!form) {
    return <Card title={t("globalSettings.title")}><p className="text-sm text-muted">{t("common.loading")}</p></Card>;
  }

  return (
    <Card title={t("globalSettings.title")} action={<Button onClick={() => void save()}>{t("common.save")}</Button>}>
      <div className="grid gap-4 lg:grid-cols-2">
        {([
          "host",
          "port",
          "proxy",
          "spoof_emulation",
          "update_source",
          "dsn",
          "data_dir",
        ] as Array<keyof GlobalSettings>).map((key) => (
          <div key={key}>
            <Label>{key}</Label>
            <Input
              value={String(form[key] ?? "")}
              onChange={(value) =>
                setForm((current) =>
                  current
                    ? {
                        ...current,
                        [key]: key === "port" ? Number(value) : value,
                      }
                    : current,
                )
              }
            />
          </div>
        ))}
      </div>
    </Card>
  );
}

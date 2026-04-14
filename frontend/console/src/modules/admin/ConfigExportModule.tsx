import { useMemo, useState } from "react";

import { useI18n } from "../../app/i18n";
import { Button, Card } from "../../components/ui";
import { apiText } from "../../lib/api";
import { authHeaders } from "../../lib/auth";

export function ConfigExportModule({
  sessionToken,
  notify,
}: {
  sessionToken: string;
  notify: (kind: "success" | "error" | "info", message: string) => void;
}) {
  const { t } = useI18n();
  const headers = useMemo(() => authHeaders(sessionToken), [sessionToken]);
  const [toml, setToml] = useState("");

  const load = async (): Promise<string | null> => {
    try {
      const next = await apiText("/admin/config/export-toml", {
        method: "POST",
        headers,
      });
      setToml(next);
      return next;
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
      return null;
    }
  };

  const download = async () => {
    const content = toml || (await load());
    if (!content) return;
    const blob = new Blob([content], { type: "application/toml;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    const stamp = new Date().toISOString().replace(/[:.]/g, "-");
    link.download = `gproxy-config-${stamp}.toml`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
  };

  return (
    <Card
      title={t("configExport.title")}
      action={
        <div className="flex flex-wrap gap-2">
          <Button onClick={() => void load()}>{t("common.export")}</Button>
          <Button variant="neutral" onClick={() => void download()}>
            {t("common.download")}
          </Button>
        </div>
      }
    >
      <pre className="overflow-auto text-xs text-muted">{toml || t("configExport.empty")}</pre>
    </Card>
  );
}

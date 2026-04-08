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

  const load = async () => {
    try {
      const next = await apiText("/admin/config/export-toml", {
        method: "POST",
        headers,
      });
      setToml(next);
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <Card title={t("configExport.title")} action={<Button onClick={() => void load()}>Export</Button>}>
      <pre className="overflow-auto text-xs text-muted">{toml || "# export not loaded"}</pre>
    </Card>
  );
}

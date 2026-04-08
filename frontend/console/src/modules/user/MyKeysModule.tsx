import { useEffect, useMemo, useState } from "react";

import { useI18n } from "../../app/i18n";
import { Badge, Button, Card } from "../../components/ui";
import { apiJson } from "../../lib/api";
import { authHeaders } from "../../lib/auth";
import type { GenerateKeyResponse, UserKeyRow } from "../../lib/types/user";

export function MyKeysModule({
  sessionToken,
  notify,
}: {
  sessionToken: string;
  notify: (kind: "success" | "error" | "info", message: string) => void;
}) {
  const { t } = useI18n();
  const [rows, setRows] = useState<UserKeyRow[]>([]);
  const headers = useMemo(() => authHeaders(sessionToken), [sessionToken]);

  const load = async () => {
    const data = await apiJson<UserKeyRow[]>("/user/keys/query", {
      method: "POST",
      headers,
    });
    setRows(data);
  };

  useEffect(() => {
    void load().catch((error) => notify("error", error instanceof Error ? error.message : String(error)));
  }, []);

  const generate = async () => {
    try {
      const generated = await apiJson<GenerateKeyResponse>("/user/keys/generate", {
        method: "POST",
        headers,
        body: JSON.stringify({}),
      });
      notify("success", generated.api_key);
      await load();
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <Card
      title={t("myKeys.title")}
      subtitle={t("myKeys.subtitle")}
      action={<Button onClick={() => void generate()}>{t("myKeys.generate")}</Button>}
    >
      <div className="record-list">
        {rows.map((row, index) => (
          <div key={`${row.api_key}-${index}`} className="record-item">
            <div className="flex flex-wrap items-center gap-2">
              {row.label ? <Badge variant="neutral">{row.label}</Badge> : null}
              <Badge variant={row.enabled ? "success" : "danger"}>
                {row.enabled ? t("common.enabled") : t("common.disabled")}
              </Badge>
            </div>
            <div className="font-mono text-xs text-text">{row.api_key}</div>
          </div>
        ))}
      </div>
    </Card>
  );
}

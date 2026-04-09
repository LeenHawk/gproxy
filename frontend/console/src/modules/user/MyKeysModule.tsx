import { useEffect, useMemo, useState } from "react";

import { useI18n } from "../../app/i18n";
import { Badge, Button, Card } from "../../components/ui";
import { apiJson, apiVoid } from "../../lib/api";
import { authHeaders } from "../../lib/auth";
import { copyText } from "../../lib/clipboard";
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

  const remove = async (id: number) => {
    try {
      await apiVoid("/user/keys/delete", {
        method: "POST",
        headers,
        body: JSON.stringify({ id }),
      });
      await load();
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  const toggleEnabled = async (row: UserKeyRow) => {
    try {
      await apiVoid("/user/keys/update-enabled", {
        method: "POST",
        headers,
        body: JSON.stringify({ id: row.id, enabled: !row.enabled }),
      });
      await load();
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  const copyKey = async (apiKey: string) => {
    try {
      await copyText(apiKey);
      notify("success", t("common.apiKeyCopied"));
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      notify("error", `${t("common.copyFailed")}: ${message}`);
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
            <div className="flex items-start justify-between gap-2">
              <div>
                <div className="flex flex-wrap items-center gap-2">
                  <button
                    type="button"
                    className="badge-button"
                    onClick={() => void toggleEnabled(row)}
                  >
                    <Badge variant={row.enabled ? "success" : "danger"}>
                      {row.enabled ? t("common.enabled") : t("common.disabled")}
                    </Badge>
                  </button>
                </div>
                <div className="font-mono text-xs text-text">{row.api_key}</div>
              </div>
              <div className="flex flex-wrap gap-2">
                <Button variant="neutral" onClick={() => void copyKey(row.api_key)}>
                  {t("common.copy")}
                </Button>
                <Button variant="danger" onClick={() => void remove(row.id)}>
                  {t("common.delete")}
                </Button>
              </div>
            </div>
          </div>
        ))}
      </div>
    </Card>
  );
}

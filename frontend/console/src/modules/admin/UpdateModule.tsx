import { useMemo, useState } from "react";

import { useI18n } from "../../app/i18n";
import { Button, Card, Input, Label } from "../../components/ui";
import { apiJson } from "../../lib/api";
import { authHeaders } from "../../lib/auth";
import type { UpdateCheckResponse, UpdateParams, UpdatePerformResponse } from "../../lib/types/admin";

export function UpdateModule({
  sessionToken,
  notify,
}: {
  sessionToken: string;
  notify: (kind: "success" | "error" | "info", message: string) => void;
}) {
  const { t } = useI18n();
  const headers = useMemo(() => authHeaders(sessionToken), [sessionToken]);
  const [tag, setTag] = useState("");
  const [check, setCheck] = useState<UpdateCheckResponse | null>(null);
  const [result, setResult] = useState<UpdatePerformResponse | null>(null);

  const checkUpdate = async () => {
    try {
      const next = await apiJson<UpdateCheckResponse>("/admin/update/check", {
        method: "POST",
        headers,
      });
      setCheck(next);
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  const performUpdate = async () => {
    try {
      const next = await apiJson<UpdatePerformResponse>("/admin/update", {
        method: "POST",
        headers,
        body: JSON.stringify({ tag: tag.trim() || null } satisfies UpdateParams),
      });
      setResult(next);
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <Card title={t("update.title")}>
      <div className="grid gap-4 lg:grid-cols-2">
        <div>
          <Label>{t("common.tag")}</Label>
          <Input value={tag} onChange={setTag} />
        </div>
      </div>
      <div className="mt-4 flex gap-2">
        <Button variant="neutral" onClick={() => void checkUpdate()}>{t("common.check")}</Button>
        <Button variant="danger" onClick={() => void performUpdate()}>{t("common.performUpdate")}</Button>
      </div>
      {check ? <pre className="mt-4 overflow-auto text-xs text-muted">{JSON.stringify(check, null, 2)}</pre> : null}
      {result ? <pre className="mt-4 overflow-auto text-xs text-muted">{JSON.stringify(result, null, 2)}</pre> : null}
    </Card>
  );
}

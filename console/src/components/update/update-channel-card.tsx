import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { ApiError } from "@/api/http";
import { instanceSettingsQuery, settingsToInput, upsertInstanceSettings } from "@/api/settings";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from "@/components/ui/select";

/** Release-channel selector for self-update; edits the (single) instance
 *  settings row's `update_channel` and auto-saves, preserving other fields. */
export function UpdateChannelCard() {
  const { t } = useTranslation("update");
  const qc = useQueryClient();
  const { data: list = [] } = useQuery(instanceSettingsQuery);
  const s = list[0];

  const [channel, setChannel] = useState("default");
  useEffect(() => {
    if (s) setChannel(s.update_channel ?? "default");
  }, [s?.id, s?.update_channel]);

  const save = useMutation({
    mutationFn: (next: string) =>
      upsertInstanceSettings({ ...settingsToInput(s!), update_channel: next === "default" ? null : next }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["instance-settings"] });
      toast.success(t("channel.saved"));
    },
    onError: (e) => toast.error(e instanceof ApiError ? e.message : String(e)),
  });

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">{t("channel.label")}</CardTitle>
      </CardHeader>
      <CardContent className="grid gap-2">
        {s ? (
          <>
            <div className="max-w-xs">
              <Select
                value={channel}
                onValueChange={(v) => { setChannel(v); save.mutate(v); }}
                disabled={save.isPending}
              >
                <SelectTrigger><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="default">{t("channel.default")}</SelectItem>
                  <SelectItem value="releases">{t("channel.releases")}</SelectItem>
                  <SelectItem value="staging">{t("channel.staging")}</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <p className="text-xs text-muted-foreground">{t("channel.hint")}</p>
          </>
        ) : (
          <p className="text-sm text-muted-foreground">{t("channel.noSettings")}</p>
        )}
      </CardContent>
    </Card>
  );
}

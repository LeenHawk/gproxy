import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { ApiError } from "@/api/http";
import { instanceSettingsQuery, settingsToInput, upsertInstanceSettings } from "@/api/settings";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import {
  Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";

/** Self-update preferences backed by the single instance-settings row. */
export function UpdateChannelCard() {
  const { t } = useTranslation("update");
  const qc = useQueryClient();
  const { data: list = [] } = useQuery(instanceSettingsQuery);
  const s = list[0];

  const [channel, setChannel] = useState("default");
  const [autoCheck, setAutoCheck] = useState(false);
  useEffect(() => {
    if (s) {
      setChannel(s.update_channel ?? "default");
      setAutoCheck(s.enable_auto_update_check);
    }
  }, [s?.id, s?.update_channel, s?.enable_auto_update_check]);

  const save = useMutation({
    mutationFn: (next: string) =>
      upsertInstanceSettings({ ...settingsToInput(s!), update_channel: next === "default" ? null : next }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["instance-settings"] });
      void qc.resetQueries({ queryKey: ["update", "check"], exact: true });
      toast.success(t("channel.saved"));
    },
    onError: (e) => toast.error(e instanceof ApiError ? e.message : String(e)),
  });

  const saveAutoCheck = useMutation({
    mutationFn: (enabled: boolean) =>
      upsertInstanceSettings({ ...settingsToInput(s!), enable_auto_update_check: enabled }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["instance-settings"] });
      toast.success(t("automaticCheck.saved"));
    },
    onError: (e) => {
      setAutoCheck(s?.enable_auto_update_check ?? false);
      toast.error(e instanceof ApiError ? e.message : String(e));
    },
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
                disabled={save.isPending || saveAutoCheck.isPending}
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
            <div className="mt-3 grid gap-1 border-t pt-4">
              <div className="flex items-center justify-between gap-4">
                <Label htmlFor="auto-update-check" className="cursor-pointer">
                  {t("automaticCheck.label")}
                </Label>
                <Switch
                  id="auto-update-check"
                  checked={autoCheck}
                  onCheckedChange={(enabled) => {
                    setAutoCheck(enabled);
                    saveAutoCheck.mutate(enabled);
                  }}
                  disabled={save.isPending || saveAutoCheck.isPending}
                />
              </div>
              <p className="text-xs text-muted-foreground">{t("automaticCheck.hint")}</p>
            </div>
          </>
        ) : (
          <p className="text-sm text-muted-foreground">{t("channel.noSettings")}</p>
        )}
      </CardContent>
    </Card>
  );
}

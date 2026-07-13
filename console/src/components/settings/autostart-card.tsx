import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { ApiError } from "@/api/http";
import { autoStartQuery, setAutoStart } from "@/api/settings";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";

export function AutoStartCard() {
  const { t } = useTranslation("settings");
  const queryClient = useQueryClient();
  const { data, error, isPending, isError } = useQuery(autoStartQuery);
  const mutation = useMutation({
    mutationFn: setAutoStart,
    onSuccess: (status) => {
      queryClient.setQueryData(autoStartQuery.queryKey, status);
      toast.success(t("autostart.saved"));
    },
    onError: () => toast.error(t("autostart.failed")),
  });

  // Edge deployments have no native host and return no endpoint. Keeping the
  // card out of that Console avoids presenting an inapplicable setting.
  if (error instanceof ApiError && (error.status === 404 || error.status === 501)) return null;

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">{t("sections.autostart")}</CardTitle>
      </CardHeader>
      <CardContent className="grid gap-2">
        {isError ? (
          <p className="text-sm text-destructive">{t("autostart.failed")}</p>
        ) : isPending || !data ? (
          <Skeleton className="h-6 w-full" />
        ) : (
          <>
            <div className="flex items-center justify-between gap-4">
              <Label htmlFor="s-autostart" className="cursor-pointer">
                {t("fields.autostart")}
              </Label>
              <Switch
                id="s-autostart"
                checked={data.enabled}
                disabled={(!data.supported && !data.enabled) || mutation.isPending}
                onCheckedChange={(enabled) => mutation.mutate(enabled)}
              />
            </div>
            <p className="text-xs text-muted-foreground">
              {data.supported
                ? t("autostart.description", { platform: data.platform })
                : (data.detail ?? t("autostart.unsupported"))}
            </p>
          </>
        )}
      </CardContent>
    </Card>
  );
}

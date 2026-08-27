import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { autostartStatus, setAutostart } from "@/api/native"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Field, FieldContent, FieldDescription, FieldLabel } from "@/components/ui/field"
import { Switch } from "@/components/ui/switch"
import { QueryState } from "@/components/query-state"

export function AutostartCard() {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const query = useQuery({ queryKey: ["autostart"], queryFn: autostartStatus })
  const mutation = useMutation({
    mutationFn: setAutostart,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["autostart"] })
      toast.success(t("settings.autostart.saved"))
    },
    onError: () => toast.error(t("settings.autostart.saveError")),
  })
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("settings.autostart.title")}</CardTitle>
        <CardDescription>{t("settings.autostart.description")}</CardDescription>
      </CardHeader>
      <CardContent>
        <QueryState loading={query.isLoading} error={query.error ? t("settings.autostart.loadError") : ""}>
          {query.data ? (
            <Field orientation="horizontal">
              <FieldContent>
                <FieldLabel htmlFor="native-autostart">{t("settings.autostart.enable")}</FieldLabel>
                <FieldDescription>
                  {query.data.supported
                    ? t("settings.autostart.platform", { platform: query.data.platform })
                    : t(`settings.autostart.detail.${query.data.detail ?? "unsupported"}`)}
                </FieldDescription>
              </FieldContent>
              <Switch id="native-autostart" checked={query.data.enabled} disabled={!query.data.supported || mutation.isPending} onCheckedChange={(enabled) => mutation.mutate({ enabled })} />
            </Field>
          ) : null}
        </QueryState>
      </CardContent>
    </Card>
  )
}

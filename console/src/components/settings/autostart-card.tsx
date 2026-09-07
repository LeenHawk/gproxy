import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { autostartStatus, setAutostart } from "@/api/native"
import { Section } from "@/components/section"
import { Field, FieldContent, FieldDescription, FieldLabel } from "@/components/ui/field"
import { Switch } from "@/components/ui/switch"
import { QueryState } from "@/components/query-state"
import type { AutostartStatusDto } from "@/generated/AutostartStatusDto"

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
  const describe = (status: AutostartStatusDto) => {
    if (status.detail) return t(`settings.autostart.detail.${status.detail}`)
    return status.supported
      ? t("settings.autostart.platform", { platform: status.platform })
      : t("settings.autostart.detail.unsupported")
  }
  return (
    <Section title={t("settings.autostart.title")} description={t("settings.autostart.description")}>
        <div>
        <QueryState loading={query.isLoading} error={query.error ? t("settings.autostart.loadError") : ""}>
          {query.data ? (
            <Field orientation="horizontal">
              <FieldContent>
                <FieldLabel htmlFor="native-autostart">{t("settings.autostart.enable")}</FieldLabel>
                <FieldDescription>{describe(query.data)}</FieldDescription>
              </FieldContent>
              <Switch id="native-autostart" checked={query.data.enabled} disabled={!query.data.supported || mutation.isPending} onCheckedChange={(enabled) => mutation.mutate({ enabled })} />
            </Field>
          ) : null}
        </QueryState>
      </div>
    </Section>
  )
}

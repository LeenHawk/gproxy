import { useId } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { portalSettings, savePortalSettings } from "@/api/portal"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Field, FieldContent, FieldDescription, FieldLabel } from "@/components/ui/field"
import { QueryState } from "@/components/query-state"
import { Switch } from "@/components/ui/switch"

export function RecentRequestsSetting() {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const switchId = useId()
  const query = useQuery({ queryKey: ["admin", "portal-settings"], queryFn: portalSettings })
  const mutation = useMutation({
    mutationFn: savePortalSettings,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["admin", "portal-settings"] })
      toast.success(t("portal.admin.recentRequests.saved"))
    },
    onError: () => toast.error(t("portal.admin.recentRequests.saveError")),
  })
  const checked = mutation.isPending
    ? mutation.variables.recent_requests_enabled
    : query.data?.recent_requests_enabled ?? false

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("portal.admin.recentRequests.title")}</CardTitle>
        <CardDescription>{t("portal.admin.recentRequests.description")}</CardDescription>
      </CardHeader>
      <CardContent>
        <QueryState loading={query.isLoading} error={query.isError ? t("portal.admin.recentRequests.loadError") : ""}>
          <Field orientation="horizontal">
            <FieldContent>
              <FieldLabel htmlFor={switchId}>{t("portal.admin.recentRequests.label")}</FieldLabel>
              <FieldDescription>{t("portal.admin.recentRequests.hint")}</FieldDescription>
            </FieldContent>
            <Switch
              id={switchId}
              checked={checked}
              disabled={mutation.isPending}
              onCheckedChange={(recent_requests_enabled) => mutation.mutate({ recent_requests_enabled })}
            />
          </Field>
        </QueryState>
      </CardContent>
    </Card>
  )
}

import { useId } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { portalSettings, savePortalSettings } from "@/api/portal"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Field, FieldContent, FieldDescription, FieldLabel } from "@/components/ui/field"
import { QueryState } from "@/components/query-state"
import { Switch } from "@/components/ui/switch"

export function PortalSettingsCard() {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const switchId = useId()
  const query = useQuery({ queryKey: ["admin", "portal-settings"], queryFn: portalSettings })
  const mutation = useMutation({
    mutationFn: savePortalSettings,
    onSuccess: async () => { await queryClient.invalidateQueries({ queryKey: ["admin", "portal-settings"] }); toast.success(t("settings.portal.saved")) },
    onError: () => toast.error(t("settings.portal.saveError")),
  })
  const checked = mutation.isPending ? mutation.variables.recent_requests_enabled : query.data?.recent_requests_enabled ?? false
  return (
    <Card>
      <CardHeader><CardTitle>{t("settings.portal.title")}</CardTitle><CardDescription>{t("settings.portal.description")}</CardDescription></CardHeader>
      <CardContent><QueryState loading={query.isLoading} error={query.isError ? t("settings.portal.loadError") : ""}><Field orientation="horizontal"><FieldContent><FieldLabel htmlFor={switchId}>{t("settings.portal.label")}</FieldLabel><FieldDescription>{t("settings.portal.hint")}</FieldDescription></FieldContent><Switch id={switchId} checked={checked} disabled={mutation.isPending} onCheckedChange={(recent_requests_enabled) => mutation.mutate({ recent_requests_enabled })} /></Field></QueryState></CardContent>
    </Card>
  )
}

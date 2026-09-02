import { useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { DownloadCloudIcon, XIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { instanceSettings } from "@/api/control"
import { updateStatus } from "@/api/native"
import { Alert, AlertAction, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { navigateAdminPath } from "@/lib/admin-route"
import { dismissUpdate, readDismissedUpdate } from "@/lib/update-banner-dismissal"

const AUTO_CHECK_STALE_TIME_MS = 15 * 60 * 1000

export function UpdateBanner() {
  const { t } = useTranslation()
  const [dismissed, setDismissed] = useState(readDismissedUpdate)
  const settings = useQuery({ queryKey: ["instance-settings"], queryFn: instanceSettings })
  const automatic = settings.data?.enable_auto_update_check === true
  const update = useQuery({
    queryKey: ["native-update"],
    queryFn: updateStatus,
    enabled: automatic,
    retry: false,
    staleTime: AUTO_CHECK_STALE_TIME_MS,
  })

  if (!automatic || !update.data?.available || dismissed === update.data.latest) return null
  const close = () => {
    dismissUpdate(update.data.latest)
    setDismissed(update.data.latest)
  }
  return (
    <Alert className="mb-6 border-state-warning/45">
      <DownloadCloudIcon aria-hidden />
      <AlertTitle>{t("update.banner.title", { version: update.data.latest })}</AlertTitle>
      <AlertDescription className="flex flex-wrap items-center gap-2">
        <span>{t("update.banner.description")}</span>
        <Button size="xs" variant="outline" onClick={() => navigateAdminPath("/admin/update")}>{t("update.banner.action")}</Button>
      </AlertDescription>
      <AlertAction>
        <Button size="icon-xs" variant="ghost" aria-label={t("update.banner.dismiss")} onClick={close}><XIcon aria-hidden /></Button>
      </AlertAction>
    </Alert>
  )
}

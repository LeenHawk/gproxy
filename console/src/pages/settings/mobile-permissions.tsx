import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { androidSetup } from "@/api/setup"
import { Button } from "@/components/ui/button"

export function AndroidPermissions() {
  const { t } = useTranslation()
  const android = androidSetup()
  const status = useQuery({ queryKey: ["desktop", "permissions"],
    queryFn: () => JSON.parse(android!.permissionStatus()) as { notifications: boolean; background: boolean },
    enabled: !!android, refetchOnWindowFocus: true, refetchInterval: 2000 })
  if (!android) return null
  return <div className="flex flex-wrap gap-3">
    <Button type="button" variant="outline" disabled={status.data?.notifications} onClick={() => android.requestNotifications()}>{t(status.data?.notifications ? "setup.notificationsGranted" : "setup.allowNotifications")}</Button>
    <Button type="button" variant="outline" disabled={status.data?.background} onClick={() => android.requestBackground()}>{t(status.data?.background ? "setup.backgroundGranted" : "setup.allowBackground")}</Button>
  </div>
}

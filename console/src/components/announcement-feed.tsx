import { CircleAlertIcon, InfoIcon, TriangleAlertIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { cn } from "@/lib/utils"

const severityStyle = {
  info: "border-state-info/45 text-state-info",
  warning: "border-state-warning/45 text-state-warning",
  critical: "border-state-critical/55 text-state-critical",
} as const

const severityIcon = {
  info: InfoIcon,
  warning: TriangleAlertIcon,
  critical: CircleAlertIcon,
} as const

export function AnnouncementFeed() {
  const { i18n } = useTranslation()
  const notifications = window.__GPROXY_ANNOUNCEMENTS__ ?? []
  if (notifications.length === 0) return null

  const locale = i18n.resolvedLanguage ?? i18n.language
  return (
    <section className="mb-6 grid gap-2" aria-live="polite">
      {notifications.flatMap((notification) => {
        const content = notification.content[locale] ?? notification.content.en
        if (!content) return []
        const Icon = severityIcon[notification.severity]
        return [
          <Alert key={notification.id} className={cn("bg-card", severityStyle[notification.severity])}>
            <Icon aria-hidden />
            <AlertTitle>{content.title}</AlertTitle>
            <AlertDescription className="whitespace-pre-wrap text-current/85">{content.body}</AlertDescription>
          </Alert>,
        ]
      })}
    </section>
  )
}

import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { CircleAlert, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { notificationsQuery, type Notification } from "@/api/notifications";
import { MarkdownContent } from "@/components/update/markdown-content";
import { Button } from "@/components/ui/button";
import { pickNotificationContent } from "@/lib/notification-locale";
import { dismissCriticalNotification, readNotificationPreferences } from "@/lib/notification-preferences";

export function CriticalNotifications() {
  const { data } = useQuery(notificationsQuery);
  const [preferences, setPreferences] = useState(readNotificationPreferences);
  const visible = (data?.notifications ?? []).filter(
    (item) => item.severity === "critical" && !preferences.dismissedCriticalIds.includes(item.id),
  );
  if (visible.length === 0) return null;
  return (
    <div>
      {visible.map((notification) => (
        <CriticalBanner
          key={notification.id}
          notification={notification}
          onDismiss={() => setPreferences((current) => dismissCriticalNotification(notification.id, current))}
        />
      ))}
    </div>
  );
}

function CriticalBanner({ notification, onDismiss }: { notification: Notification; onDismiss: () => void }) {
  const { i18n, t } = useTranslation();
  const content = pickNotificationContent(notification.content, i18n.resolvedLanguage ?? i18n.language);
  return (
    <div role="alert" className="border-b border-red-300 bg-red-50 text-red-950 dark:border-red-900 dark:bg-red-950 dark:text-red-100">
      <div className="flex items-start gap-3 px-4 py-3 md:px-6">
        <CircleAlert className="mt-0.5 size-4 shrink-0" aria-hidden />
        <div className="min-w-0 flex-1">
          <p className="font-semibold">{content.title}</p>
          <MarkdownContent markdown={content.body} className="mt-1 text-sm" />
        </div>
        <Button type="button" size="icon-sm" variant="ghost" className="shrink-0 hover:bg-red-100 dark:hover:bg-red-900" aria-label={t("actions.close")} onClick={onDismiss}>
          <X className="size-4" aria-hidden />
        </Button>
      </div>
    </div>
  );
}

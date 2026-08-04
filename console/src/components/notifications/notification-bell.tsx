import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Bell, CircleAlert, Info, TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import { notificationsQuery, type Notification, type NotificationSeverity } from "@/api/notifications";
import { MarkdownContent } from "@/components/update/markdown-content";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { pickNotificationContent } from "@/lib/notification-locale";
import { markNotificationsRead, readNotificationPreferences } from "@/lib/notification-preferences";
import { cn } from "@/lib/utils";

const severityStyle: Record<NotificationSeverity, string> = {
  info: "text-blue-600 dark:text-blue-400",
  warning: "text-amber-600 dark:text-amber-400",
  critical: "text-red-600 dark:text-red-400",
};

export function NotificationBell() {
  const { data } = useQuery(notificationsQuery);
  const notifications = data?.notifications ?? [];
  const [open, setOpen] = useState(false);
  const [preferences, setPreferences] = useState(readNotificationPreferences);
  const { t } = useTranslation();
  const unread = notifications.filter((item) => !preferences.readIds.includes(item.id)).length;

  if (notifications.length === 0) return null;

  const handleOpen = (next: boolean) => {
    setOpen(next);
    if (next) setPreferences((current) => markNotificationsRead(notifications.map((item) => item.id), current));
  };

  return (
    <Popover open={open} onOpenChange={handleOpen}>
      <PopoverTrigger asChild>
        <Button type="button" variant="ghost" size="icon" className="relative" aria-label={t("notifications.label")}>
          <Bell className="size-5" aria-hidden />
          {unread > 0 && (
            <span className="absolute -right-0.5 -top-0.5 min-w-4 rounded-full bg-red-600 px-1 text-center text-[10px] font-semibold leading-4 text-white">
              {unread > 99 ? "99+" : unread}
            </span>
          )}
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="max-h-[70vh] w-[min(26rem,calc(100vw-2rem))] overflow-y-auto p-0">
        <div className="border-b px-4 py-3 font-semibold">{t("notifications.title")}</div>
        <div className="divide-y">
          {notifications.map((notification) => <NotificationItem key={notification.id} notification={notification} />)}
        </div>
      </PopoverContent>
    </Popover>
  );
}

function NotificationItem({ notification }: { notification: Notification }) {
  const { i18n, t } = useTranslation();
  const language = i18n.resolvedLanguage ?? i18n.language;
  const content = pickNotificationContent(notification.content, language);
  const Icon = notification.severity === "critical" ? CircleAlert : notification.severity === "warning" ? TriangleAlert : Info;
  return (
    <article className="p-4">
      <div className="flex gap-3">
        <Icon className={cn("mt-0.5 size-4 shrink-0", severityStyle[notification.severity])} aria-hidden />
        <div className="min-w-0 flex-1">
          <h3 className="font-medium">{content.title}</h3>
          <time className="text-xs text-muted-foreground" dateTime={notification.published_at}>
            {t("notifications.publishedAt", { date: new Intl.DateTimeFormat(language, { dateStyle: "medium" }).format(new Date(notification.published_at)) })}
          </time>
          <MarkdownContent markdown={content.body} className="mt-2 text-sm" />
        </div>
      </div>
    </article>
  );
}

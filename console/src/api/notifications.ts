import { queryOptions } from "@tanstack/react-query";
import { api } from "./http";

export type NotificationSeverity = "info" | "warning" | "critical";

export interface NotificationContent {
  title: string;
  body: string;
}

export interface Notification {
  id: string;
  severity: NotificationSeverity;
  published_at: string;
  expires_at?: string;
  affects?: string;
  content: Record<string, NotificationContent>;
}

interface NotificationsResponse {
  notifications: Notification[];
}

export const notificationsQuery = queryOptions({
  queryKey: ["notifications"],
  queryFn: () => api<NotificationsResponse>("/admin/notifications"),
  staleTime: 30 * 60 * 1000,
  retry: false,
});

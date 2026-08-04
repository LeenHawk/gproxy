import type { NotificationContent } from "@/api/notifications";

export function pickNotificationContent(
  content: Record<string, NotificationContent>,
  language: string,
): NotificationContent {
  if (content[language]) return content[language];
  const normalized = language.toLowerCase();
  if (normalized === "zh-tw" && content["zh-CN"]) return content["zh-CN"];
  if (normalized.startsWith("zh") && content["zh-CN"]) return content["zh-CN"];
  return content.en;
}

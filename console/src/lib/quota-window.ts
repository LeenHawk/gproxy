export type QuotaWindow = "day" | "week" | "month" | "five_hour" | "seven_day";

interface QuotaWindowUsage {
  day_used: string;
  day_anchor: number;
  week_used: string;
  week_anchor: number;
  month_used: string;
  month_anchor: number;
  five_hour_used: string;
  five_hour_anchor: number;
  seven_day_used: string;
  seven_day_anchor: number;
}

export function dayKey(now = Date.now()): number {
  return Math.floor(now / 1000 / 86_400);
}

export function weekKey(now = Date.now()): number {
  return Math.floor((dayKey(now) + 3) / 7);
}

export function monthKey(now = Date.now()): number {
  const date = new Date(now);
  return date.getUTCFullYear() * 12 + date.getUTCMonth();
}

export function effectiveWindowUsed(
  quota: QuotaWindowUsage,
  window: QuotaWindow,
  now = Date.now(),
): string {
  switch (window) {
    case "day":
      return quota.day_anchor === dayKey(now) ? quota.day_used : "0";
    case "week":
      return quota.week_anchor === weekKey(now) ? quota.week_used : "0";
    case "month":
      return quota.month_anchor === monthKey(now) ? quota.month_used : "0";
    case "five_hour":
      return quota.five_hour_anchor > 0 && now / 1000 < quota.five_hour_anchor + 5 * 3600
        ? quota.five_hour_used : "0";
    case "seven_day":
      return quota.seven_day_anchor > 0 && now / 1000 < quota.seven_day_anchor + 7 * 86_400
        ? quota.seven_day_used : "0";
  }
}

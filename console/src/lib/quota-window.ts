export type QuotaWindow = "day" | "week" | "month";

interface QuotaWindowUsage {
  day_used: string;
  day_anchor: number;
  week_used: string;
  week_anchor: number;
  month_used: string;
  month_anchor: number;
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
  }
}

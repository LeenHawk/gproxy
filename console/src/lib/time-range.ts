/** Unix-second time range. Both ends optional — `undefined` means unbounded. */
export interface TimeRange {
  from?: number;
  to?: number;
}

/** A range with both ends pinned — what the rollup charts require. */
export type BoundedTimeRange = Required<TimeRange>;

export type Granularity = "hour" | "day";

/** Spans up to this length chart per-hour; longer ones chart per-day. */
const HOUR_GRANULARITY_MAX_SECS = 3 * 86_400;

export type QuickFillKey = "today" | "yesterday" | "thisWeek" | "thisMonth";

const pad = (n: number) => String(n).padStart(2, "0");

/**
 * Format for `<input type="datetime-local">`, i.e. `YYYY-MM-DDTHH:mm` in the
 * viewer's local zone. Deliberately hand-built: `toISOString().slice(0, 16)`
 * would emit UTC and shift every value by the zone offset.
 */
export function toLocalInput(unixSecs: number): string {
  const d = new Date(unixSecs * 1000);
  return (
    `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}` +
    `T${pad(d.getHours())}:${pad(d.getMinutes())}`
  );
}

/** Inverse of `toLocalInput`. Empty or unparseable input yields `undefined`. */
export function fromLocalInput(value: string): number | undefined {
  if (!value) return undefined;
  const ms = new Date(value).getTime();
  return Number.isNaN(ms) ? undefined : Math.floor(ms / 1000);
}

export function pickGranularity(from: number, to: number): Granularity {
  return to - from <= HOUR_GRANULARITY_MAX_SECS ? "hour" : "day";
}

function startOfDay(d: Date): Date {
  const copy = new Date(d);
  copy.setHours(0, 0, 0, 0);
  return copy;
}

const unix = (d: Date) => Math.floor(d.getTime() / 1000);

/**
 * Calendar-aligned shortcuts that merely *fill in* the two endpoints — unlike
 * the rolling `24h`/`7d` presets they replace, the resulting range stays fully
 * editable and is never re-derived from "now". Week starts Monday (ISO 8601).
 */
export function quickFills(
  now = new Date(),
): { key: QuickFillKey; range: BoundedTimeRange }[] {
  const today = startOfDay(now);
  const to = unix(now);

  const yesterday = new Date(today);
  yesterday.setDate(yesterday.getDate() - 1);

  const weekStart = new Date(today);
  // getDay(): 0 = Sunday → 6 days back to the preceding Monday.
  weekStart.setDate(weekStart.getDate() - ((weekStart.getDay() + 6) % 7));

  const monthStart = new Date(today);
  monthStart.setDate(1);

  return [
    { key: "today", range: { from: unix(today), to } },
    // Yesterday is a closed day: `at_to` is inclusive server-side, so stop one
    // second before midnight rather than at it.
    {
      key: "yesterday",
      range: { from: unix(yesterday), to: unix(today) - 1 },
    },
    { key: "thisWeek", range: { from: unix(weekStart), to } },
    { key: "thisMonth", range: { from: unix(monthStart), to } },
  ];
}

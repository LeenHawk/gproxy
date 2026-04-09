// ---------------------------------------------------------------------------
// Shared parsing helpers
// ---------------------------------------------------------------------------

function pad2(value: number): string {
  return String(value).padStart(2, "0");
}

function isLeapYear(year: number): boolean {
  return (year % 4 === 0 && year % 100 !== 0) || year % 400 === 0;
}

/// Convert (year, day-of-year) into (month, day-of-month). Returns null on
/// out-of-range input. Used by `fromOffsetDateTimeTuple` to decode the
/// `time::OffsetDateTime` JSON tuple shape that the backend serializes.
function ordinalToMonthDay(year: number, ordinal: number): { month: number; day: number } | null {
  if (!Number.isInteger(ordinal) || ordinal <= 0 || ordinal > (isLeapYear(year) ? 366 : 365)) {
    return null;
  }
  const monthDays = [31, isLeapYear(year) ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
  let remain = ordinal;
  for (let month = 1; month <= monthDays.length; month += 1) {
    const days = monthDays[month - 1];
    if (remain <= days) {
      return { month, day: remain };
    }
    remain -= days;
  }
  return null;
}

function formatGmtOffset(date: Date): string {
  const offsetMinutes = -date.getTimezoneOffset();
  const sign = offsetMinutes >= 0 ? "+" : "-";
  const abs = Math.abs(offsetMinutes);
  const hours = Math.floor(abs / 60);
  const minutes = abs % 60;
  return minutes === 0 ? `${sign}${hours}` : `${sign}${hours}:${pad2(minutes)}`;
}

/// Decode `time::OffsetDateTime`'s default JSON shape — a 9-tuple
/// `[year, day_of_year, hour, minute, second, nanosecond, off_h, off_m, off_s]`.
function fromOffsetDateTimeTuple(value: unknown): Date | null {
  if (!Array.isArray(value) || value.length < 9) {
    return null;
  }
  const items = value.map((item) => Number(item));
  if (items.some((item) => Number.isNaN(item))) {
    return null;
  }
  const [year, ordinal, hour, minute, second, nanosecond, offsetHours, offsetMinutes, offsetSeconds] = items;
  const md = ordinalToMonthDay(year, ordinal);
  if (!md) {
    return null;
  }
  const utcMs = Date.UTC(
    year,
    md.month - 1,
    md.day,
    hour,
    minute,
    second,
    Math.trunc(nanosecond / 1_000_000),
  );
  const offsetMs = (offsetHours * 3600 + offsetMinutes * 60 + offsetSeconds) * 1000;
  return new Date(utcMs - offsetMs);
}

/// Best-effort decoder for "looks like a unix timestamp" strings/numbers.
/// Auto-detects seconds / millis / micros / nanos by magnitude.
function fromUnixLike(value: string | number): Date | null {
  const raw = typeof value === "number" ? value.toString() : value.trim();
  if (!raw || !/^-?\d+$/.test(raw)) {
    return null;
  }
  let int: bigint;
  try {
    int = BigInt(raw);
  } catch {
    return null;
  }
  const abs = int < 0n ? -int : int;
  let millis: bigint;
  if (abs >= 1_000_000_000_000_000_000n) {
    millis = int / 1_000_000n;
  } else if (abs >= 1_000_000_000_000_000n) {
    millis = int / 1_000n;
  } else if (abs >= 1_000_000_000_000n) {
    millis = int;
  } else {
    millis = int * 1_000n;
  }
  const asNumber = Number(millis);
  return Number.isFinite(asNumber) ? new Date(asNumber) : null;
}

function parseToDate(value: unknown): Date | null {
  const tupleDate = fromOffsetDateTimeTuple(value);
  if (tupleDate) {
    return tupleDate;
  }
  if (typeof value === "string") {
    const unixLike = fromUnixLike(value);
    if (unixLike) {
      return unixLike;
    }
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? null : date;
  }
  if (typeof value === "number") {
    return fromUnixLike(value);
  }
  return null;
}

// ---------------------------------------------------------------------------
// Public helpers
// ---------------------------------------------------------------------------

export function formatTimestamp(value?: string | null): string {
  if (!value) {
    return "—";
  }
  const date = parseToDate(value);
  if (!date) {
    return value;
  }
  return date.toLocaleString();
}

export function formatUnixMs(value?: number | null): string {
  if (value === null || value === undefined) {
    return "—";
  }
  return new Date(value).toLocaleString();
}

/// Render a backend `at` field (string, OffsetDateTime tuple, or unix-like
/// number) as `YYYY-MM-DD:HH-mm GMT±H:MM`. Used by the request and usage
/// log tables for compact, sortable display.
export function formatAtForViewer(value: unknown): string {
  const date = parseToDate(value);
  if (!date) {
    if (typeof value === "string") {
      return value;
    }
    if (Array.isArray(value)) {
      return value.join(",");
    }
    return String(value ?? "");
  }
  const year = date.getFullYear();
  const month = pad2(date.getMonth() + 1);
  const day = pad2(date.getDate());
  const hour = pad2(date.getHours());
  const minute = pad2(date.getMinutes());
  return `${year}-${month}-${day}:${hour}-${minute} GMT${formatGmtOffset(date)}`;
}

/// Convert a backend `at` value to unix milliseconds for cursor pagination.
/// Returns null when the value can't be parsed.
export function parseAtToUnixMs(value: unknown): number | null {
  const date = parseToDate(value);
  if (!date) {
    return null;
  }
  const ms = date.getTime();
  return Number.isFinite(ms) ? ms : null;
}

/// Parse a user-typed datetime input (`YYYY-MM-DD HH:mm`, with `/` and `T`
/// separators also accepted) to unix milliseconds in the local timezone.
/// Falls back to `new Date(raw)` for ISO strings. Used by request/usage
/// time-range filters where the user types a wall-clock time.
export function parseDateTimeLocalToUnixMs(value: string): number | null {
  const raw = value.trim();
  if (!raw) {
    return null;
  }
  const normalized = raw.replaceAll("/", "-");
  const manual = normalized.match(/^(\d{4})-(\d{1,2})-(\d{1,2})(?:[ T](\d{1,2})[:-](\d{1,2}))?$/);
  if (manual) {
    const year = Number(manual[1]);
    const month = Number(manual[2]);
    const day = Number(manual[3]);
    const hour = Number(manual[4] ?? "0");
    const minute = Number(manual[5] ?? "0");
    if (
      Number.isNaN(year) ||
      Number.isNaN(month) ||
      Number.isNaN(day) ||
      Number.isNaN(hour) ||
      Number.isNaN(minute) ||
      month < 1 ||
      month > 12 ||
      day < 1 ||
      day > 31 ||
      hour < 0 ||
      hour > 23 ||
      minute < 0 ||
      minute > 59
    ) {
      return null;
    }
    const local = new Date(year, month - 1, day, hour, minute, 0, 0);
    if (
      local.getFullYear() !== year ||
      local.getMonth() !== month - 1 ||
      local.getDate() !== day ||
      local.getHours() !== hour ||
      local.getMinutes() !== minute
    ) {
      return null;
    }
    return local.getTime();
  }
  const date = new Date(raw);
  return Number.isNaN(date.getTime()) ? null : date.getTime();
}

//! Numbers and instants, formatted for the reader's locale.
//!
//! Every timestamp v4 exchanges is Unix **milliseconds** and every money
//! amount is a decimal **string** — that is the DTO convention, and these
//! helpers take exactly those two shapes rather than converting at each call
//! site, where one forgotten `* 1000` reads as "1970" and one `Number()` too
//! many quietly rounds a cost.

export function formatCost(value: string | number, locale: string) {
  const amount = typeof value === "number" ? value : Number(value)
  const small = Math.abs(amount) < 0.01 && amount !== 0
  return new Intl.NumberFormat(locale, {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: small ? 4 : 2,
    maximumFractionDigits: small ? 6 : 2,
  }).format(Number.isFinite(amount) ? amount : 0)
}

export function formatCount(value: number, locale: string) {
  return new Intl.NumberFormat(locale, { notation: value >= 100_000 ? "compact" : "standard" }).format(value)
}

export function formatNumber(value: number | string, locale: string) {
  const amount = typeof value === "number" ? value : Number(value)
  return new Intl.NumberFormat(locale, { maximumFractionDigits: 2 }).format(Number.isFinite(amount) ? amount : 0)
}

export function formatPercent(value: number, locale: string) {
  return new Intl.NumberFormat(locale, { style: "percent", maximumFractionDigits: 1 }).format(value)
}

/** A Unix-millisecond instant. `null` stays `null` so a caller renders its own dash. */
export function formatInstant(value: number | null | undefined, locale: string) {
  if (value == null) return null
  return new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short" }).format(new Date(value))
}

/** A duration given in milliseconds, at the coarsest unit that still reads. */
export function formatDurationMs(value: number, locale: string) {
  if (value < 1_000) return `${Math.round(value)} ms`
  const seconds = value / 1_000
  if (seconds < 90) return new Intl.NumberFormat(locale, { style: "unit", unit: "second", maximumFractionDigits: 1 }).format(seconds)
  const minutes = seconds / 60
  if (minutes < 90) return new Intl.NumberFormat(locale, { style: "unit", unit: "minute", maximumFractionDigits: 0 }).format(minutes)
  const hours = minutes / 60
  if (hours < 48) return new Intl.NumberFormat(locale, { style: "unit", unit: "hour", maximumFractionDigits: 0 }).format(hours)
  return new Intl.NumberFormat(locale, { style: "unit", unit: "day", maximumFractionDigits: 0 }).format(hours / 24)
}

/** `1732492800000` back into the `datetime-local` spelling an input wants. */
export function toLocalInput(value: number | null | undefined) {
  if (value == null) return ""
  const at = new Date(value - new Date(value).getTimezoneOffset() * 60_000)
  return at.toISOString().slice(0, 16)
}

/** The inverse, tolerant of the empty string a cleared input leaves behind. */
export function fromLocalInput(value: string): number | null {
  if (!value.trim()) return null
  const at = new Date(value)
  return Number.isNaN(at.getTime()) ? null : at.getTime()
}

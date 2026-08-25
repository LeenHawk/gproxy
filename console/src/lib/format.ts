export function formatCost(value: string | number, locale: string) {
  const amount = typeof value === "number" ? value : Number(value)
  return new Intl.NumberFormat(locale, {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: amount < 0.01 ? 4 : 2,
    maximumFractionDigits: amount < 0.01 ? 6 : 2,
  }).format(Number.isFinite(amount) ? amount : 0)
}

export function formatCount(value: number, locale: string) {
  return new Intl.NumberFormat(locale, { notation: value >= 100_000 ? "compact" : "standard" }).format(value)
}

export function formatNumber(value: number, locale: string) {
  return new Intl.NumberFormat(locale, { maximumFractionDigits: 2 }).format(value)
}

export function formatPercent(value: number, locale: string) {
  return new Intl.NumberFormat(locale, { style: "percent", maximumFractionDigits: 1 }).format(value)
}

export function formatInstant(value: number | null, locale: string) {
  if (value == null) return null
  return new Intl.DateTimeFormat(locale, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(value * 1000))
}

export function formatDuration(seconds: number, locale: string) {
  const minutes = Math.max(1, Math.round(seconds / 60))
  if (minutes < 60) return new Intl.NumberFormat(locale, { style: "unit", unit: "minute" }).format(minutes)
  const hours = Math.round(minutes / 60)
  if (hours < 48) return new Intl.NumberFormat(locale, { style: "unit", unit: "hour" }).format(hours)
  return new Intl.NumberFormat(locale, { style: "unit", unit: "day" }).format(Math.round(hours / 24))
}

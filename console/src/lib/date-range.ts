export type DateRange = { start: number; end: number }

export function validDateRange(range: DateRange) {
  return Number.isFinite(range.start) && Number.isFinite(range.end) && range.start < range.end
}

export function toLocalDateTime(value: number) {
  const instant = new Date(value * 1000)
  return new Date(instant.getTime() - instant.getTimezoneOffset() * 60_000).toISOString().slice(0, 16)
}

export function fromLocalDateTime(value: string) {
  return Math.floor(new Date(value).getTime() / 1000)
}

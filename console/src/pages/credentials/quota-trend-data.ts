import type { QuotaObservationDto } from "@/generated/sdk"

export type QuotaTrendPoint = { time: number; percent: number | null; cycleId: string | null }

/** Keep actual observation times; unknown readings and cycle boundaries break the line. */
export function quotaTrendPoints(rows: QuotaObservationDto[], windowId: string): QuotaTrendPoint[] {
  const points: QuotaTrendPoint[] = []
  let previous: QuotaObservationDto | undefined
  for (const row of [...rows].sort((a, b) => a.observedAtMs - b.observedAtMs)) {
    if (!row.snapshot || typeof row.snapshot !== "object") continue
    const snapshot = row.snapshot as { id?: string; used_percent?: string | null; used?: string | null; limit?: string | null; unlimited?: boolean | null }
    if (snapshot.id !== windowId) continue
    const used = snapshot.used == null ? NaN : Number(snapshot.used)
    const limit = snapshot.limit == null ? NaN : Number(snapshot.limit)
    const percent = snapshot.used_percent != null ? Number(snapshot.used_percent)
      : !snapshot.unlimited && limit > 0 ? used / limit * 100 : NaN
    // Without a cycle identity, retain the point but do not guess which period it belongs to.
    if (previous && (!row.cycleId || row.cycleId !== previous.cycleId)) {
      points.push({ time: row.observedAtMs, percent: null, cycleId: null })
    }
    points.push({ time: row.observedAtMs, percent: Number.isFinite(percent) ? percent : null, cycleId: row.cycleId })
    previous = row
  }
  return points
}

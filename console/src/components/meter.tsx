import { cn } from "@/lib/utils"

function meterColor(percent: number) {
  if (percent >= 90) return "var(--state-critical)"
  if (percent >= 70) return "var(--state-warning)"
  return "var(--state-healthy)"
}

/* Severity-colored fill on a lighter track of the same color. Color never
   carries alone — callers render the value as text beside the bar. */
export function Meter({ percent, className }: { percent: number; className?: string }) {
  const clamped = Math.min(100, Math.max(0, percent))
  const color = meterColor(clamped)
  return (
    <div
      aria-hidden
      className={cn("h-1.5 w-full rounded-full", className)}
      style={{ background: `color-mix(in srgb, ${color} 18%, transparent)` }}
    >
      <div className="h-full rounded-full" style={{ width: `${clamped}%`, background: color }} />
    </div>
  )
}

import type { CSSProperties } from "react";
import { cn } from "@/lib/utils";

/** Fixed categorical slot order (see globals.css); never cycle past it —
 *  callers cap their series count instead. */
export const CHART_SERIES = [
  "var(--chart-1)",
  "var(--chart-2)",
  "var(--chart-3)",
  "var(--chart-4)",
  "var(--chart-5)",
] as const;

export function seriesColor(slot: number): string {
  return CHART_SERIES[slot % CHART_SERIES.length];
}

export const CHART_TOOLTIP_STYLE: CSSProperties = {
  background: "var(--popover)",
  border: "1px solid var(--border)",
  borderRadius: "0.5rem",
  boxShadow: "0 4px 12px rgb(0 0 0 / 0.08)",
  fontSize: 12,
};

/** Recessive axes: hairline-free ticks in muted ink, no axis/tick lines. */
export const CHART_AXIS = {
  tickLine: false,
  axisLine: false,
  tick: { fontSize: 11, fill: "var(--muted-foreground)" },
} as const;

export interface LegendChip {
  label: string;
  color: string;
}

/** Identity legend: colored dot + text-token label (text never wears the series color). */
export function LegendChips({ items, className }: { items: LegendChip[]; className?: string }) {
  if (items.length < 2) return null;
  return (
    <div className={cn("flex flex-wrap items-center gap-x-3 gap-y-1", className)}>
      {items.map((item) => (
        <span key={item.label} className="inline-flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
          <span aria-hidden className="size-2 shrink-0 rounded-full" style={{ background: item.color }} />
          <span className="truncate">{item.label}</span>
        </span>
      ))}
    </div>
  );
}

function meterColor(percent: number): string {
  if (percent >= 90) return "var(--destructive)";
  if (percent >= 70) return "var(--chart-4)";
  return "var(--chart-1)";
}

/** Utilization meter: severity-colored fill on a lighter track of the same color.
 *  Always render the percentage as text next to it — color never carries alone. */
export function Meter({ percent, className }: { percent: number; className?: string }) {
  const clamped = Math.min(100, Math.max(0, percent));
  const color = meterColor(clamped);
  return (
    <div
      aria-hidden
      className={cn("h-1.5 w-full rounded-full", className)}
      style={{ background: `color-mix(in srgb, ${color} 18%, transparent)` }}
    >
      <div
        className="h-full rounded-full"
        style={{ width: `${clamped}%`, background: color }}
      />
    </div>
  );
}

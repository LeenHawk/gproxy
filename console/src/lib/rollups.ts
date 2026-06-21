import type { UsageRollup } from "../api/usage";

export interface ChartPoint {
  t: number;
  requests: number;
  input_tokens: number;
  output_tokens: number;
  cost: number;
}

/**
 * Aggregate day-granularity UsageRollup rows (which contain one row per
 * dimension combination per bucket) into a single ChartPoint per bucket_start.
 * Sums all dimension rows for the same bucket; parses the Decimal cost string
 * to a number (for display/charting only — never store or send back).
 */
export function aggregateRollups(rows: UsageRollup[]): ChartPoint[] {
  const map = new Map<number, ChartPoint>();

  for (const row of rows) {
    const t = row.bucket_start;
    const existing = map.get(t);
    const costNum = parseFloat(row.cost) || 0;
    if (existing) {
      existing.requests += row.requests;
      existing.input_tokens += row.input_tokens;
      existing.output_tokens += row.output_tokens;
      existing.cost += costNum;
    } else {
      map.set(t, {
        t,
        requests: row.requests,
        input_tokens: row.input_tokens,
        output_tokens: row.output_tokens,
        cost: costNum,
      });
    }
  }

  return Array.from(map.values()).sort((a, b) => a.t - b.t);
}

import type { CredentialUsageDay, UsageTokenTotals } from "@/api/credentials";

export const EMPTY_USAGE_TOTALS: UsageTokenTotals = {
  requests: 0,
  input_tokens: 0,
  output_tokens: 0,
  image_output_tokens: 0,
  cache_read_tokens: 0,
  cache_creation_tokens: 0,
  total_tokens: 0,
  cost_usd: "0",
};

export function compareDecimalStrings(left: string, right: string): number {
  return (Number(left) || 0) - (Number(right) || 0);
}

export function isPositiveDecimal(value: string): boolean {
  return (Number(value) || 0) > 0;
}

export function decimalToChartNumber(value: string): number {
  const number = Number(value);
  return Number.isFinite(number) ? number : 0;
}

export function sumUsageTotals(totals: readonly UsageTokenTotals[]): UsageTokenTotals {
  const summed = totals.reduce(
    (result, item) => ({
      requests: result.requests + item.requests,
      input_tokens: result.input_tokens + item.input_tokens,
      output_tokens: result.output_tokens + item.output_tokens,
      image_output_tokens: result.image_output_tokens + item.image_output_tokens,
      cache_read_tokens: result.cache_read_tokens + item.cache_read_tokens,
      cache_creation_tokens: result.cache_creation_tokens + item.cache_creation_tokens,
      total_tokens: result.total_tokens + item.total_tokens,
      cost: result.cost + (Number(item.cost_usd) || 0),
    }),
    {
      requests: 0,
      input_tokens: 0,
      output_tokens: 0,
      image_output_tokens: 0,
      cache_read_tokens: 0,
      cache_creation_tokens: 0,
      total_tokens: 0,
      cost: 0,
    },
  );

  const { cost, ...counts } = summed;
  return { ...counts, cost_usd: String(cost) };
}

export function utcDayStart(unixSeconds: number): number {
  const date = new Date(unixSeconds * 1000);
  return Date.UTC(date.getUTCFullYear(), date.getUTCMonth(), date.getUTCDate()) / 1000;
}

/**
 * Returns a stable seven-day series, filling dates with no traffic. The server
 * owns the totals; this only normalizes sparse responses for charts and cards.
 */
export function normalizeLastSevenDays(
  days: readonly CredentialUsageDay[],
  nowSeconds = Math.floor(Date.now() / 1000),
): CredentialUsageDay[] {
  const byDay = new Map(days.map((day) => [utcDayStart(day.day_start), day.totals]));
  const today = utcDayStart(nowSeconds);

  return Array.from({ length: 7 }, (_, index) => {
    const dayStart = today - (6 - index) * 86_400;
    return {
      day_start: dayStart,
      totals: byDay.get(dayStart) ?? EMPTY_USAGE_TOTALS,
    };
  });
}

export function formatUsageCount(value: number): string {
  return new Intl.NumberFormat(undefined, {
    notation: "compact",
    maximumFractionDigits: 2,
  }).format(value);
}

export function formatUsageUsd(value: string, locales?: Intl.LocalesArgument): string {
  const amount = Number(value);
  if (!value.trim() || !Number.isFinite(amount)) return `${value.trim() || "—"} USD`;

  const maximumFractionDigits = amount !== 0 && Math.abs(amount) < 0.01 ? 6 : 2;
  return new Intl.NumberFormat(locales, {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: 2,
    maximumFractionDigits,
  }).format(amount);
}

export interface CategorizedTokenTotals {
  input_tokens: number;
  output_tokens: number;
  image_output_tokens: number;
  cache_read_tokens: number;
  cache_creation_5m_tokens: number;
  cache_creation_30m_tokens: number;
  cache_creation_1h_tokens: number;
}

export function categorizedTotalTokens(totals: CategorizedTokenTotals): number {
  return totals.input_tokens
    + totals.output_tokens
    + totals.image_output_tokens
    + totals.cache_read_tokens
    + totals.cache_creation_5m_tokens
    + totals.cache_creation_30m_tokens
    + totals.cache_creation_1h_tokens;
}

import { scopeAll } from "../../lib/scope";
import type { UsageQuery, UsageQueryRow } from "../../lib/types/shared";

export type MyUsageSummary = {
  displayed: number;
  inputTokens: number;
  outputTokens: number;
  cachedTokens: number;
};

export const MY_USAGE_PAGE_SIZE = 50;

export function buildMyUsageQuery(offset = 0): UsageQuery {
  return {
    provider_id: scopeAll<number>(),
    credential_id: scopeAll<number>(),
    channel: scopeAll<string>(),
    model: scopeAll<string>(),
    user_id: scopeAll<number>(),
    user_key_id: scopeAll<number>(),
    ...(offset > 0 ? { offset } : {}),
    limit: MY_USAGE_PAGE_SIZE,
  };
}

export function summarizeUsageRows(rows: UsageQueryRow[]): MyUsageSummary {
  return rows.reduce(
    (summary, row) => ({
      displayed: summary.displayed + 1,
      inputTokens: summary.inputTokens + (row.input_tokens ?? 0),
      outputTokens: summary.outputTokens + (row.output_tokens ?? 0),
      cachedTokens:
        summary.cachedTokens +
        (row.cache_read_input_tokens ?? 0) +
        (row.cache_creation_input_tokens ?? 0) +
        (row.cache_creation_input_tokens_5min ?? 0) +
        (row.cache_creation_input_tokens_1h ?? 0),
    }),
    {
      displayed: 0,
      inputTokens: 0,
      outputTokens: 0,
      cachedTokens: 0,
    },
  );
}

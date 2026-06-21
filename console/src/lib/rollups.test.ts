import { describe, expect, it } from "vitest";
import { aggregateRollups } from "./rollups";
import type { UsageRollup } from "../api/usage";

function makeRow(overrides: Partial<UsageRollup>): UsageRollup {
  return {
    id: 1,
    granularity: "day",
    bucket_start: 1_700_000_000,
    provider_id: null,
    org_id: null,
    team_id: null,
    user_id: null,
    route_name: null,
    model: null,
    requests: 1,
    input_tokens: 100,
    output_tokens: 50,
    cost: "0.001",
    ...overrides,
  };
}

describe("aggregateRollups", () => {
  it("returns empty array for empty input", () => {
    expect(aggregateRollups([])).toEqual([]);
  });

  it("sums multiple dimension rows within the same bucket", () => {
    const rows = [
      makeRow({ id: 1, requests: 5, input_tokens: 100, output_tokens: 50, cost: "0.010" }),
      makeRow({ id: 2, requests: 3, input_tokens: 200, output_tokens: 80, cost: "0.020", model: "claude-3-sonnet" }),
      makeRow({ id: 3, requests: 2, input_tokens: 50, output_tokens: 20, cost: "0.005", provider_id: 2 }),
    ];
    const result = aggregateRollups(rows);
    expect(result).toHaveLength(1);
    expect(result[0].requests).toBe(10);
    expect(result[0].input_tokens).toBe(350);
    expect(result[0].output_tokens).toBe(150);
    expect(result[0].cost).toBeCloseTo(0.035);
  });

  it("parses Decimal cost strings to numbers", () => {
    const rows = [
      makeRow({ cost: "1.234567890" }),
      makeRow({ cost: "0.000001", id: 2 }),
    ];
    const result = aggregateRollups(rows);
    expect(result).toHaveLength(1);
    expect(typeof result[0].cost).toBe("number");
    expect(result[0].cost).toBeCloseTo(1.234568890, 9);
  });

  it("sorts results by bucket_start ascending across multiple buckets", () => {
    const t1 = 1_700_000_000;
    const t2 = 1_700_086_400;
    const t3 = 1_700_172_800;
    const rows = [
      makeRow({ bucket_start: t3, requests: 3, cost: "0.003" }),
      makeRow({ bucket_start: t1, requests: 1, cost: "0.001" }),
      makeRow({ bucket_start: t2, requests: 2, cost: "0.002" }),
    ];
    const result = aggregateRollups(rows);
    expect(result.map((p) => p.t)).toEqual([t1, t2, t3]);
    expect(result.map((p) => p.requests)).toEqual([1, 2, 3]);
  });
});

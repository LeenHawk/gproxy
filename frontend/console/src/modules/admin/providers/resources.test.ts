import { describe, expect, it } from "vitest";

import {
  filterModelsForProvider,
  nextResourceId,
  providerOptionLabel,
} from "./resources";

describe("provider resources helpers", () => {
  it("computes the next resource id from existing rows", () => {
    expect(nextResourceId([{ id: 2 }, { id: 9 }, { id: 4 }])).toBe("10");
  });

  it("filters models by provider id (includes aliases)", () => {
    expect(
      filterModelsForProvider(
        [
          { id: 1, provider_id: 10, model_id: "a", enabled: true, price_tiers: [], alias_of: null },
          { id: 2, provider_id: 20, model_id: "b", enabled: true, price_tiers: [], alias_of: null },
          { id: 3, provider_id: 20, model_id: "alias-b", enabled: true, price_tiers: [], alias_of: 2 },
        ] as never,
        20,
      ).map((row) => row.id),
    ).toEqual([2, 3]);
  });

  it("formats provider option labels with id", () => {
    expect(providerOptionLabel({ id: 7, name: "demo" } as never)).toBe("demo (#7)");
  });
});

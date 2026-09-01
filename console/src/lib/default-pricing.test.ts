import { describe, expect, it } from "vitest"
import type { DefaultPriceCatalogDto } from "@/generated/DefaultPriceCatalogDto"
import { findDefaultPrice } from "./default-pricing"

const catalog = {
  schema_version: 1,
  source: {
    catalog: "test",
    fetched_at: "2026-09-02T00:00:00Z",
    total_models: 2,
    supported_output_models: 2,
    dynamic_price_models: 0,
    included_models: 2,
    embedding_models: 0,
    rerank_models: 0,
    image_output_priced_models: 0,
  },
  price_rules: [
    { model_id: "test/model", model_pattern: "*model*", tiers: null, priority: 2, rates: [] },
    { model_id: "test/model-pro", model_pattern: "*model-pro*", tiers: null, priority: 1, rates: [] },
  ],
} satisfies DefaultPriceCatalogDto

describe("default pricing", () => {
  it("matches case-insensitively and prefers the longest model fragment", () => {
    expect(findDefaultPrice(catalog, "TEST/MODEL-PRO:FAST")?.model_id).toBe("test/model-pro")
  })
})

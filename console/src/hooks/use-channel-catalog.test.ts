import { describe, expect, it } from "vitest";
import { ApiError } from "@/api/http";
import { CHANNELS, type ChannelMeta } from "@/lib/channel-meta";
import { resolveChannelCatalog } from "./use-channel-catalog";

describe("resolveChannelCatalog", () => {
  it("keeps loading distinct from a real failure", () => {
    expect(resolveChannelCatalog(undefined, null)).toMatchObject({
      catalog: [], availability: "loading", authoritative: false,
    });

    const error = new ApiError(500, "internal", "catalog failed");
    expect(resolveChannelCatalog(undefined, error)).toEqual({
      catalog: [], availability: "error", authoritative: false, error,
    });
  });

  it.each([404, 405])("uses display-only compatibility metadata for HTTP %i", (status) => {
    const state = resolveChannelCatalog(
      undefined,
      new ApiError(status, "not_found", "old backend"),
    );
    expect(state.catalog).toBe(CHANNELS);
    expect(state.availability).toBe("legacy");
    expect(state.authoritative).toBe(false);
  });

  it("treats even an empty successful response as authoritative", () => {
    const data: ChannelMeta[] = [];
    expect(resolveChannelCatalog(data, null)).toEqual({
      catalog: data, availability: "ready", authoritative: true, error: null,
    });
  });

  it("keeps cached successful data authoritative when a refetch fails", () => {
    const data: ChannelMeta[] = [];
    expect(resolveChannelCatalog(data, new ApiError(500, "internal", "refetch failed"))).toEqual({
      catalog: data, availability: "ready", authoritative: true, error: null,
    });
  });
});

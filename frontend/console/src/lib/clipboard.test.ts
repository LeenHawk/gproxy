import { describe, expect, it, vi } from "vitest";

import { copyText } from "./clipboard";

describe("copyText", () => {
  it("writes text with the provided clipboard implementation", async () => {
    const writeText = vi.fn(async () => {});

    await expect(copyText("gp_test_key", { writeText })).resolves.toBeUndefined();
    expect(writeText).toHaveBeenCalledWith("gp_test_key");
  });

  it("rejects when the Clipboard API is unavailable", async () => {
    await expect(copyText("gp_test_key", undefined)).rejects.toThrow("Clipboard API unavailable");
  });
});

import { describe, expect, it } from "vitest";
import { dismissUpdate, readDismissedUpdate } from "./update-banner-dismissal";

function memoryStorage() {
  const values = new Map<string, string>();
  return {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => { values.set(key, value); },
  };
}

describe("update banner dismissal", () => {
  it("persists the dismissed build identity", () => {
    const storage = memoryStorage();
    expect(readDismissedUpdate(storage)).toBeNull();

    dismissUpdate("deadbeefcafe", storage);

    expect(readDismissedUpdate(storage)).toBe("deadbeefcafe");
  });

  it("replaces the old identity so a future build can be dismissed separately", () => {
    const storage = memoryStorage();
    dismissUpdate("old-build", storage);
    dismissUpdate("new-build", storage);
    expect(readDismissedUpdate(storage)).toBe("new-build");
  });
});

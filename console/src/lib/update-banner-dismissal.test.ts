import { describe, expect, it } from "vitest"
import { dismissUpdate, readDismissedUpdate } from "./update-banner-dismissal"

function memoryStorage() {
  const values = new Map<string, string>()
  return {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => { values.set(key, value) },
  }
}

describe("update banner dismissal", () => {
  it("dismisses only the selected update identity", () => {
    const storage = memoryStorage()
    dismissUpdate("3.0.0-alpha.1", storage)
    expect(readDismissedUpdate(storage)).toBe("3.0.0-alpha.1")
    dismissUpdate("3.0.0-alpha.2", storage)
    expect(readDismissedUpdate(storage)).toBe("3.0.0-alpha.2")
  })
})

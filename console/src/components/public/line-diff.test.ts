import { describe, expect, it } from "vitest"
import { lineDiff } from "@/components/public/line-diff"

describe("lineDiff", () => {
  it("keeps identical input untouched", () => {
    const diff = lineDiff("{\n  \"a\": 1\n}", "{\n  \"a\": 1\n}")
    expect(diff.rewritten).toBe(0)
    expect(diff.kept).toBe(3)
    expect(diff.left.every((line) => line.kind === "same")).toBe(true)
  })

  it("marks inserted and replaced lines on the right and consumed lines on the left", () => {
    const before = ["{", "  \"a\": 1,", "  \"b\": 2", "}"].join("\n")
    const after = ["{", "  \"a\": 1,", "  \"max\": 9,", "  \"c\": 2", "}"].join("\n")
    const diff = lineDiff(before, after)
    expect(diff.right.map((line) => line.kind)).toEqual(["same", "same", "changed", "changed", "same"])
    expect(diff.left.map((line) => line.kind)).toEqual(["same", "same", "changed", "same"])
    expect(diff.rewritten).toBe(2)
    expect(diff.kept).toBe(3)
  })

  it("marks everything changed when nothing lines up", () => {
    const diff = lineDiff("x\ny", "p\nq\nr")
    expect(diff.right.every((line) => line.kind === "changed")).toBe(true)
    expect(diff.left.every((line) => line.kind === "changed")).toBe(true)
    expect(diff.rewritten).toBe(3)
  })
})

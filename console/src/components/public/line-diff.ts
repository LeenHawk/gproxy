export type DiffKind = "same" | "changed"
export type DiffLine = { text: string; kind: DiffKind }
export type LineDiff = { left: Array<DiffLine>; right: Array<DiffLine>; rewritten: number; kept: number }

export function lineDiff(before: string, after: string): LineDiff {
  const a = before.split("\n")
  const b = after.split("\n")
  const table = Array.from({ length: a.length + 1 }, () => new Uint16Array(b.length + 1))
  for (let i = a.length - 1; i >= 0; i--) {
    for (let j = b.length - 1; j >= 0; j--) {
      table[i][j] = a[i] === b[j] ? table[i + 1][j + 1] + 1 : Math.max(table[i + 1][j], table[i][j + 1])
    }
  }

  const left: Array<DiffLine> = []
  const right: Array<DiffLine> = []
  let i = 0
  let j = 0
  while (i < a.length && j < b.length) {
    if (a[i] === b[j]) {
      left.push({ text: a[i], kind: "same" })
      right.push({ text: b[j], kind: "same" })
      i++
      j++
    } else if (table[i + 1][j] >= table[i][j + 1]) {
      left.push({ text: a[i++], kind: "changed" })
    } else {
      right.push({ text: b[j++], kind: "changed" })
    }
  }
  while (i < a.length) left.push({ text: a[i++], kind: "changed" })
  while (j < b.length) right.push({ text: b[j++], kind: "changed" })

  const kept = right.filter((line) => line.kind === "same").length
  return { left, right, rewritten: right.length - kept, kept }
}

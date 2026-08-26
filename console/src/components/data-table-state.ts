import { useState } from "react"

type StoredColumns = { version: 1; hidden: Array<string> }

function readHidden(storageKey: string) {
  try {
    const value = JSON.parse(window.localStorage.getItem(storageKey) ?? "null") as StoredColumns | null
    return value?.version === 1 ? new Set(value.hidden) : new Set<string>()
  } catch {
    return new Set<string>()
  }
}

export function useColumnVisibility(storageKey: string) {
  const [hidden, setHidden] = useState(() => readHidden(storageKey))
  const toggle = (key: string) => {
    setHidden((current) => {
      const next = new Set(current)
      if (next.has(key)) next.delete(key)
      else next.add(key)
      try {
        window.localStorage.setItem(storageKey, JSON.stringify({ version: 1, hidden: [...next] } satisfies StoredColumns))
      } catch {
        // Column visibility remains usable for this session when storage is unavailable.
      }
      return next
    })
  }
  return { hidden, toggle }
}

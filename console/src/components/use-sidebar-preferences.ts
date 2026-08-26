import { useState } from "react"

const STORAGE_KEY = "gproxy.sidebar.preferences"
const defaultWidth = 240
const minWidth = 192
const maxWidth = 384
type StoredSidebar = { version: 1; width: number; collapsed: boolean }

function readStored(): StoredSidebar {
  try {
    const value = JSON.parse(window.localStorage.getItem(STORAGE_KEY) ?? "null") as StoredSidebar | null
    if (value?.version === 1) return { ...value, width: Math.min(maxWidth, Math.max(minWidth, value.width)) }
  } catch {
    // Default preferences keep the shell usable when storage is unavailable.
  }
  return { version: 1, width: defaultWidth, collapsed: false }
}

export function useSidebarPreferences() {
  const [value, setValue] = useState(readStored)
  const update = (next: StoredSidebar) => {
    setValue(next)
    try {
      window.localStorage.setItem(STORAGE_KEY, JSON.stringify(next))
    } catch {
      // The updated layout remains active for this session.
    }
  }
  return {
    ...value,
    minWidth,
    maxWidth,
    setWidth: (width: number) => update({ ...value, width: Math.min(maxWidth, Math.max(minWidth, width)) }),
    resetWidth: () => update({ ...value, width: defaultWidth }),
    toggle: () => update({ ...value, collapsed: !value.collapsed }),
  }
}

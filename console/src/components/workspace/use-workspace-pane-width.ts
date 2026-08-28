import { useCallback, useEffect, useState } from "react"

const DEFAULT_WIDTH = 384
const MIN_WIDTH = 220
const MAX_WIDTH = 480

function viewportWidth() {
  return typeof window === "undefined" ? 1280 : window.innerWidth
}

function maxWidthForViewport(width: number) {
  return width < 768 ? MAX_WIDTH : Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, Math.floor(width * 0.45)))
}

function clampWidth(width: number, maxWidth: number) {
  return Math.min(maxWidth, Math.max(MIN_WIDTH, Math.round(width)))
}

function storedWidth(storageKey: string, maxWidth: number) {
  if (typeof window === "undefined") return clampWidth(DEFAULT_WIDTH, maxWidth)
  try {
    const value = Number(window.localStorage.getItem(storageKey))
    return clampWidth(Number.isFinite(value) && value > 0 ? value : DEFAULT_WIDTH, maxWidth)
  } catch {
    return clampWidth(DEFAULT_WIDTH, maxWidth)
  }
}

export function useWorkspacePaneWidth(storageKey: string) {
  const [windowWidth, setWindowWidth] = useState(viewportWidth)
  const maxWidth = maxWidthForViewport(windowWidth)
  const [width, setWidthState] = useState(() => storedWidth(storageKey, maxWidth))

  useEffect(() => {
    const onResize = () => {
      const next = viewportWidth()
      setWindowWidth(next)
      setWidthState((current) => clampWidth(current, maxWidthForViewport(next)))
    }
    window.addEventListener("resize", onResize)
    return () => window.removeEventListener("resize", onResize)
  }, [])

  useEffect(() => {
    try {
      window.localStorage.setItem(storageKey, String(width))
    } catch {
      return
    }
  }, [storageKey, width])

  const setWidth = useCallback((next: number) => setWidthState(clampWidth(next, maxWidth)), [maxWidth])
  const resetWidth = useCallback(() => setWidth(DEFAULT_WIDTH), [setWidth])
  return { width, minWidth: MIN_WIDTH, maxWidth, setWidth, resetWidth }
}

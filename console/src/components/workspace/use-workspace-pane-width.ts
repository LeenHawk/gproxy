import { useCallback, useEffect, useState } from "react"

type Options = { defaultWidth?: number; minWidth?: number; maxWidth?: number }

function viewportWidth() {
  return typeof window === "undefined" ? 1280 : window.innerWidth
}

function maxWidthForViewport(width: number, minWidth: number, maxWidth: number) {
  return width < 768 ? maxWidth : Math.max(minWidth, Math.min(maxWidth, Math.floor(width * 0.45)))
}

function clampWidth(width: number, minWidth: number, maxWidth: number) {
  return Math.min(maxWidth, Math.max(minWidth, Math.round(width)))
}

function storedWidth(storageKey: string, defaultWidth: number, minWidth: number, maxWidth: number) {
  if (typeof window === "undefined") return clampWidth(defaultWidth, minWidth, maxWidth)
  try {
    const value = Number(window.localStorage.getItem(storageKey))
    return clampWidth(Number.isFinite(value) && value > 0 ? value : defaultWidth, minWidth, maxWidth)
  } catch {
    return clampWidth(defaultWidth, minWidth, maxWidth)
  }
}

export function useWorkspacePaneWidth(storageKey: string, { defaultWidth = 384, minWidth = 220, maxWidth: widthLimit = 480 }: Options = {}) {
  const [windowWidth, setWindowWidth] = useState(viewportWidth)
  const maxWidth = maxWidthForViewport(windowWidth, minWidth, widthLimit)
  const [width, setWidthState] = useState(() => storedWidth(storageKey, defaultWidth, minWidth, maxWidth))

  useEffect(() => {
    const onResize = () => {
      const next = viewportWidth()
      setWindowWidth(next)
      setWidthState((current) => clampWidth(current, minWidth, maxWidthForViewport(next, minWidth, widthLimit)))
    }
    window.addEventListener("resize", onResize)
    return () => window.removeEventListener("resize", onResize)
  }, [minWidth, widthLimit])

  useEffect(() => {
    try {
      window.localStorage.setItem(storageKey, String(width))
    } catch {
      return
    }
  }, [storageKey, width])

  const setWidth = useCallback((next: number) => setWidthState(clampWidth(next, minWidth, maxWidth)), [minWidth, maxWidth])
  const resetWidth = useCallback(() => setWidth(defaultWidth), [defaultWidth, setWidth])
  return { width, minWidth, maxWidth, setWidth, resetWidth }
}

import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react"
import { useTranslation } from "react-i18next"
import { cn } from "@/lib/utils"

/** Desktop columns with a persistent leading width; mobile children keep their own layout. */
export function ResizableColumns({ storageKey, initialWidth, minWidth, maxWidth, contentMinWidth, label, className, handleClassName, children }: {
  storageKey: string
  initialWidth: number
  minWidth: number
  maxWidth: number
  contentMinWidth: number
  label: string
  className?: string
  handleClassName?: string
  children: [ReactNode, ReactNode]
}) {
  const { t } = useTranslation()
  const container = useRef<HTMLDivElement>(null)
  const drag = useRef<{ x: number; width: number } | null>(null)
  const [limit, setLimit] = useState(maxWidth)
  const [width, setWidth] = useState(() => {
    try {
      const saved = Number(localStorage.getItem(storageKey))
      if (Number.isFinite(saved) && saved >= minWidth) return Math.min(maxWidth, saved)
    } catch { /* Storage may be unavailable. */ }
    return initialWidth
  })
  const actualWidth = Math.min(width, limit)
  useEffect(() => {
    const element = container.current!
    const observer = new ResizeObserver(() => {
      setLimit(Math.max(minWidth, Math.min(maxWidth, element.clientWidth - contentMinWidth - 4)))
    })
    observer.observe(element)
    return () => observer.disconnect()
  }, [minWidth, maxWidth, contentMinWidth])
  function resize(value: number) {
    const next = Math.round(Math.max(minWidth, Math.min(limit, value)))
    setWidth(next)
    try { localStorage.setItem(storageKey, String(next)) } catch { /* Resizing still works without storage. */ }
  }
  return <div ref={container} className={cn("lg:grid lg:grid-cols-[var(--leading-width)_4px_minmax(0,1fr)]", className)} style={{ "--leading-width": `${actualWidth}px` } as CSSProperties}>
    {children[0]}
    <div role="region" aria-label={t("shell.resizePanel", { name: label })} className="hidden lg:block">
    <div
      role="separator" aria-orientation="vertical" aria-label={label}
      aria-valuemin={minWidth} aria-valuemax={limit} aria-valuenow={actualWidth}
      title={t("shell.resizeHint")} tabIndex={0}
      className={cn("relative h-full touch-none select-none bg-border/50 outline-none hover:bg-primary/50 focus-visible:bg-primary active:bg-primary lg:cursor-col-resize before:absolute before:inset-y-0 before:-inset-x-1", handleClassName)}
      onPointerDown={event => {
        if (event.button !== 0) return
        event.preventDefault()
        event.currentTarget.focus()
        event.currentTarget.setPointerCapture(event.pointerId)
        drag.current = { x: event.clientX, width: actualWidth }
      }}
      onPointerMove={event => { if (drag.current) resize(drag.current.width + event.clientX - drag.current.x) }}
      onPointerUp={event => {
        drag.current = null
        if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId)
      }}
      onPointerCancel={() => { drag.current = null }}
      onLostPointerCapture={() => { drag.current = null }}
      onDoubleClick={() => resize(initialWidth)}
      onKeyDown={event => {
        const next = event.key === "ArrowLeft" ? actualWidth - 16 : event.key === "ArrowRight" ? actualWidth + 16 : event.key === "Home" ? minWidth : event.key === "End" ? limit : event.key === "Enter" ? initialWidth : null
        if (next !== null) { event.preventDefault(); resize(next) }
      }}
    />
    </div>
    {children[1]}
  </div>
}

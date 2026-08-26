import type { KeyboardEvent, PointerEvent } from "react"

export function SidebarResizeHandle({ label, width, minWidth, maxWidth, onWidth, onReset }: { label: string; width: number; minWidth: number; maxWidth: number; onWidth: (width: number) => void; onReset: () => void }) {
  const move = (event: PointerEvent<HTMLDivElement>) => {
    if (event.currentTarget.hasPointerCapture(event.pointerId)) onWidth(event.clientX)
  }
  const key = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "ArrowLeft") onWidth(width - 8)
    else if (event.key === "ArrowRight") onWidth(width + 8)
    else if (event.key === "Home") onWidth(minWidth)
    else if (event.key === "End") onWidth(maxWidth)
    else return
    event.preventDefault()
  }
  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      aria-valuemin={minWidth}
      aria-valuemax={maxWidth}
      aria-valuenow={width}
      tabIndex={0}
      className="absolute inset-y-0 -right-1 hidden w-2 cursor-col-resize touch-none bg-transparent focus-visible:bg-ring/30 lg:block"
      onDoubleClick={onReset}
      onKeyDown={key}
      onPointerDown={(event) => event.currentTarget.setPointerCapture(event.pointerId)}
      onPointerMove={move}
    />
  )
}

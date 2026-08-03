import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";

interface VerticalResizeHandleProps {
  label: string;
  width: number;
  minWidth: number;
  maxWidth: number;
  onWidthChange: (width: number) => void;
  onReset: () => void;
}

export function VerticalResizeHandle({
  label,
  width,
  minWidth,
  maxWidth,
  onWidthChange,
  onReset,
}: VerticalResizeHandleProps) {
  const [dragging, setDragging] = useState(false);
  const draggingRef = useRef(false);
  const start = useRef({ x: 0, width: 0 });
  const previousBodyStyle = useRef({ cursor: "", userSelect: "" });

  const finishDrag = () => {
    if (!draggingRef.current) return;
    draggingRef.current = false;
    setDragging(false);
    document.body.style.cursor = previousBodyStyle.current.cursor;
    document.body.style.userSelect = previousBodyStyle.current.userSelect;
  };

  useEffect(() => () => {
    if (!draggingRef.current) return;
    document.body.style.cursor = previousBodyStyle.current.cursor;
    document.body.style.userSelect = previousBodyStyle.current.userSelect;
  }, []);

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    start.current = { x: event.clientX, width };
    previousBodyStyle.current = {
      cursor: document.body.style.cursor,
      userSelect: document.body.style.userSelect,
    };
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
    draggingRef.current = true;
    setDragging(true);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    let next: number | undefined;
    if (event.key === "ArrowLeft") next = width - 16;
    if (event.key === "ArrowRight") next = width + 16;
    if (event.key === "Home") next = minWidth;
    if (event.key === "End") next = maxWidth;
    if (next === undefined) return;
    event.preventDefault();
    onWidthChange(next);
  };

  return (
    <div
      role="separator"
      aria-label={label}
      aria-orientation="vertical"
      aria-valuemin={minWidth}
      aria-valuemax={maxWidth}
      aria-valuenow={width}
      tabIndex={0}
      data-dragging={dragging}
      className="group relative z-10 -mx-1 hidden w-2 shrink-0 touch-none cursor-col-resize outline-none focus-visible:bg-primary/10 md:block"
      onDoubleClick={onReset}
      onKeyDown={onKeyDown}
      onPointerDown={onPointerDown}
      onPointerMove={(event) => {
        if (draggingRef.current) onWidthChange(start.current.width + event.clientX - start.current.x);
      }}
      onPointerUp={(event) => {
        if (event.currentTarget.hasPointerCapture(event.pointerId)) {
          event.currentTarget.releasePointerCapture(event.pointerId);
        }
        finishDrag();
      }}
      onPointerCancel={finishDrag}
      onLostPointerCapture={finishDrag}
    >
      <span
        aria-hidden
        className="absolute inset-y-0 left-1/2 w-px -translate-x-1/2 bg-border/60 transition-colors group-hover:bg-primary/50 group-data-[dragging=true]:bg-primary"
      />
    </div>
  );
}

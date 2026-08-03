import { useCallback, useEffect, useRef, useState } from "react";

export const WORKSPACE_PANE_DEFAULT_WIDTH = 288;
export const WORKSPACE_PANE_MIN_WIDTH = 220;
export const WORKSPACE_PANE_MAX_WIDTH = 480;
const MAX_VIEWPORT_FRACTION = 0.45;

function viewportWidth(): number {
  return typeof window === "undefined" ? 1280 : window.innerWidth;
}

function maxWidthForViewport(width: number): number {
  if (width < 768) return WORKSPACE_PANE_MAX_WIDTH;
  return Math.max(
    WORKSPACE_PANE_MIN_WIDTH,
    Math.min(WORKSPACE_PANE_MAX_WIDTH, Math.floor(width * MAX_VIEWPORT_FRACTION)),
  );
}

function clampWidth(width: number, maxWidth: number): number {
  return Math.min(maxWidth, Math.max(WORKSPACE_PANE_MIN_WIDTH, Math.round(width)));
}

function storedWidth(storageKey: string, maxWidth: number): number {
  if (typeof window === "undefined") return clampWidth(WORKSPACE_PANE_DEFAULT_WIDTH, maxWidth);
  try {
    const value = Number(window.localStorage.getItem(storageKey));
    return clampWidth(Number.isFinite(value) && value > 0 ? value : WORKSPACE_PANE_DEFAULT_WIDTH, maxWidth);
  } catch {
    return clampWidth(WORKSPACE_PANE_DEFAULT_WIDTH, maxWidth);
  }
}

export function useWorkspacePaneWidth(storageKey: string) {
  const [windowWidth, setWindowWidth] = useState(viewportWidth);
  const maxWidth = maxWidthForViewport(windowWidth);
  const [width, setWidthState] = useState(() => storedWidth(storageKey, maxWidth));
  const storageKeyRef = useRef(storageKey);

  useEffect(() => {
    const onResize = () => setWindowWidth(viewportWidth());
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  useEffect(() => {
    setWidthState((current) => clampWidth(current, maxWidth));
  }, [maxWidth]);

  useEffect(() => {
    if (storageKeyRef.current !== storageKey) {
      storageKeyRef.current = storageKey;
      setWidthState(storedWidth(storageKey, maxWidth));
      return;
    }
    try {
      window.localStorage.setItem(storageKey, String(width));
    } catch {
      // Storage may be unavailable in privacy-restricted contexts; resizing still works.
    }
  }, [maxWidth, storageKey, width]);

  const setWidth = useCallback(
    (next: number) => setWidthState(clampWidth(next, maxWidth)),
    [maxWidth],
  );
  const resetWidth = useCallback(
    () => setWidth(WORKSPACE_PANE_DEFAULT_WIDTH),
    [setWidth],
  );

  return {
    width,
    minWidth: WORKSPACE_PANE_MIN_WIDTH,
    maxWidth,
    setWidth,
    resetWidth,
  };
}

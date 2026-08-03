import { useCallback, useEffect, useState } from "react";

export const SIDEBAR_WIDTH_STORAGE_KEY = "gproxy.sidebar.width";
export const SIDEBAR_DEFAULT_WIDTH = 240;
export const SIDEBAR_MIN_WIDTH = 180;
export const SIDEBAR_MAX_WIDTH = 400;
const MAX_VIEWPORT_FRACTION = 0.35;

function viewportWidth(): number {
  return typeof window === "undefined" ? 1280 : window.innerWidth;
}

function maxWidthForViewport(width: number): number {
  if (width < 768) return SIDEBAR_MAX_WIDTH;
  return Math.max(
    SIDEBAR_MIN_WIDTH,
    Math.min(SIDEBAR_MAX_WIDTH, Math.floor(width * MAX_VIEWPORT_FRACTION)),
  );
}

function clampWidth(width: number, maxWidth: number): number {
  return Math.min(maxWidth, Math.max(SIDEBAR_MIN_WIDTH, Math.round(width)));
}

function initialWidth(maxWidth: number): number {
  if (typeof window === "undefined") return clampWidth(SIDEBAR_DEFAULT_WIDTH, maxWidth);
  try {
    const stored = Number(window.localStorage.getItem(SIDEBAR_WIDTH_STORAGE_KEY));
    return clampWidth(Number.isFinite(stored) && stored > 0 ? stored : SIDEBAR_DEFAULT_WIDTH, maxWidth);
  } catch {
    return clampWidth(SIDEBAR_DEFAULT_WIDTH, maxWidth);
  }
}

export function useSidebarWidth() {
  const [windowWidth, setWindowWidth] = useState(viewportWidth);
  const maxWidth = maxWidthForViewport(windowWidth);
  const [width, setWidthState] = useState(() => initialWidth(maxWidth));

  useEffect(() => {
    const onResize = () => setWindowWidth(viewportWidth());
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  useEffect(() => {
    setWidthState((current) => clampWidth(current, maxWidth));
  }, [maxWidth]);

  useEffect(() => {
    try {
      window.localStorage.setItem(SIDEBAR_WIDTH_STORAGE_KEY, String(width));
    } catch {
      // Storage may be unavailable in privacy-restricted contexts; resizing still works.
    }
  }, [width]);

  const setWidth = useCallback(
    (next: number) => setWidthState(clampWidth(next, maxWidth)),
    [maxWidth],
  );

  return {
    width,
    minWidth: SIDEBAR_MIN_WIDTH,
    maxWidth,
    setWidth,
    resetWidth: useCallback(() => setWidth(SIDEBAR_DEFAULT_WIDTH), [setWidth]),
  };
}

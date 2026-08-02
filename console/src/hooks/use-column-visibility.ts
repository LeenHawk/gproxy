import { useState } from "react";

function loadHiddenColumns(storageKey: string, defaults: string[]): Set<string> {
  if (typeof window === "undefined") return new Set(defaults);

  try {
    const stored = window.localStorage.getItem(storageKey);
    if (stored === null) return new Set(defaults);
    const parsed: unknown = JSON.parse(stored);
    return new Set(
      Array.isArray(parsed)
        ? parsed.filter((key): key is string => typeof key === "string")
        : defaults,
    );
  } catch {
    return new Set(defaults);
  }
}

export function useColumnVisibility(
  storageKey: string | undefined,
  defaultHidden: string[] = [],
) {
  const [hidden, setHidden] = useState<Set<string>>(() =>
    storageKey ? loadHiddenColumns(storageKey, defaultHidden) : new Set(),
  );

  function setVisible(key: string, visible: boolean) {
    if (!storageKey) return;
    setHidden((current) => {
      const next = new Set(current);
      if (visible) next.delete(key);
      else next.add(key);
      try {
        window.localStorage.setItem(storageKey, JSON.stringify([...next]));
      } catch {
        // Keep the in-memory preference when storage is unavailable.
      }
      return next;
    });
  }

  return { hidden, setVisible };
}

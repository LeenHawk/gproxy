const STORAGE_KEY = "gproxy:update-banner-dismissed";

type StorageLike = Pick<Storage, "getItem" | "setItem">;

function browserStorage(): StorageLike | undefined {
  try {
    return window.localStorage;
  } catch {
    return undefined;
  }
}

/** Latest update identity dismissed in this browser. */
export function readDismissedUpdate(storage = browserStorage()): string | null {
  try {
    return storage?.getItem(STORAGE_KEY) ?? null;
  } catch {
    return null;
  }
}

/** Dismiss only this update; a later identity will make the banner reappear. */
export function dismissUpdate(identity: string, storage = browserStorage()): void {
  try {
    storage?.setItem(STORAGE_KEY, identity);
  } catch {
    // Storage may be disabled or full; the in-memory component state still
    // dismisses the banner for the current page lifetime.
  }
}

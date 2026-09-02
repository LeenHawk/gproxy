const STORAGE_KEY = "gproxy:update-banner-dismissed"

type StorageLike = Pick<Storage, "getItem" | "setItem">

function browserStorage(): StorageLike | undefined {
  try {
    return window.localStorage
  } catch {
    return undefined
  }
}

export function readDismissedUpdate(storage = browserStorage()): string | null {
  try {
    return storage?.getItem(STORAGE_KEY) ?? null
  } catch {
    return null
  }
}

export function dismissUpdate(identity: string, storage = browserStorage()) {
  try {
    storage?.setItem(STORAGE_KEY, identity)
  } catch {
    // The current component still dismisses the banner when browser storage is unavailable.
  }
}

const STORAGE_KEY = "gproxy:notification-preferences";

type StorageLike = Pick<Storage, "getItem" | "setItem">;

export interface NotificationPreferences {
  readIds: string[];
  dismissedCriticalIds: string[];
}

const EMPTY: NotificationPreferences = { readIds: [], dismissedCriticalIds: [] };

function browserStorage(): StorageLike | undefined {
  try {
    return window.localStorage;
  } catch {
    return undefined;
  }
}

export function readNotificationPreferences(storage = browserStorage()): NotificationPreferences {
  try {
    const value = JSON.parse(storage?.getItem(STORAGE_KEY) ?? "null");
    return {
      readIds: Array.isArray(value?.readIds) ? value.readIds.filter(isString) : [],
      dismissedCriticalIds: Array.isArray(value?.dismissedCriticalIds)
        ? value.dismissedCriticalIds.filter(isString)
        : [],
    };
  } catch {
    return { ...EMPTY };
  }
}

export function markNotificationsRead(
  ids: string[],
  current = readNotificationPreferences(),
  storage = browserStorage(),
): NotificationPreferences {
  const stored = readNotificationPreferences(storage);
  return save({
    readIds: unique([...stored.readIds, ...current.readIds, ...ids]),
    dismissedCriticalIds: unique([...stored.dismissedCriticalIds, ...current.dismissedCriticalIds]),
  }, storage);
}

export function dismissCriticalNotification(
  id: string,
  current = readNotificationPreferences(),
  storage = browserStorage(),
): NotificationPreferences {
  const stored = readNotificationPreferences(storage);
  return save({
    readIds: unique([...stored.readIds, ...current.readIds]),
    dismissedCriticalIds: unique([...stored.dismissedCriticalIds, ...current.dismissedCriticalIds, id]),
  }, storage);
}

function save(value: NotificationPreferences, storage?: StorageLike): NotificationPreferences {
  try {
    storage?.setItem(STORAGE_KEY, JSON.stringify(value));
  } catch {
    // In-memory state still applies when storage is unavailable.
  }
  return value;
}

const isString = (value: unknown): value is string => typeof value === "string";
const unique = (values: string[]) => [...new Set(values)];

type LoginSessionPayload = {
  user_id: number;
  session_token: string;
  expires_in_secs: number;
  is_admin: boolean;
};

export type StoredSession = {
  userId: number;
  sessionToken: string;
  isAdmin: boolean;
  expiresAt: number;
};

const SESSION_STORAGE_KEY = "gproxy_console_session";

type StorageLike = {
  getItem: (key: string) => string | null;
  setItem: (key: string, value: string) => void;
  removeItem: (key: string) => void;
};

function createMemoryStorage(): StorageLike {
  const map = new Map<string, string>();
  return {
    getItem(key) {
      return map.get(key) ?? null;
    },
    setItem(key, value) {
      map.set(key, value);
    },
    removeItem(key) {
      map.delete(key);
    },
  };
}

const memoryStorage = createMemoryStorage();

function storage(): StorageLike {
  if (typeof window !== "undefined" && window.localStorage) {
    return window.localStorage;
  }
  return memoryStorage;
}

export function saveSession(payload: LoginSessionPayload): StoredSession {
  const session: StoredSession = {
    userId: payload.user_id,
    sessionToken: payload.session_token,
    isAdmin: payload.is_admin,
    expiresAt: Date.now() + payload.expires_in_secs * 1000,
  };
  storage().setItem(SESSION_STORAGE_KEY, JSON.stringify(session));
  return session;
}

export function loadSession(): StoredSession | null {
  const raw = storage().getItem(SESSION_STORAGE_KEY);
  if (!raw) {
    return null;
  }
  try {
    const session = JSON.parse(raw) as StoredSession;
    if (typeof session.expiresAt !== "number" || session.expiresAt <= Date.now()) {
      clearSession();
      return null;
    }
    return session;
  } catch {
    clearSession();
    return null;
  }
}

export function clearSession() {
  storage().removeItem(SESSION_STORAGE_KEY);
}

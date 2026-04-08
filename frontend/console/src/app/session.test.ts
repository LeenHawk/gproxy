import { describe, expect, it } from "vitest";

import { clearSession, loadSession, saveSession } from "./session";

describe("session storage", () => {
  it("persists login response and derives expiry", () => {
    clearSession();
    saveSession({
      user_id: 1,
      session_token: "sess-demo",
      expires_in_secs: 60,
      is_admin: true,
    });
    const session = loadSession();
    expect(session?.sessionToken).toBe("sess-demo");
    expect(session?.isAdmin).toBe(true);
    expect(session?.expiresAt).toBeGreaterThan(Date.now());
  });
});

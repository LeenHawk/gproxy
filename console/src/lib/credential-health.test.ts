import { describe, expect, it } from "vitest";
import {
  countCurrentUnhealthyModels,
  currentCredentialStatuses,
  latestCurrentCredentialStatus,
  type DatedCredentialHealthLike,
} from "./credential-health";

describe("credential health status helpers", () => {
  it("ignores expired rate-limit and auth cooldowns", () => {
    const rows: DatedCredentialHealthLike[] = [
      {
        health_kind: "rate_limited",
        health_json: { open_until: 100 },
        updated_at: 20,
      },
      {
        health_kind: "auth_dead",
        health_json: { open_until: 99 },
        updated_at: 30,
      },
      {
        health_kind: "breaker",
        health_json: { open_until: 102 },
        updated_at: 25,
      },
      {
        health_kind: "recovered",
        health_json: null,
        updated_at: 10,
      },
    ];

    expect(currentCredentialStatuses(rows, 101).map((row) => row.health_kind)).toEqual([
      "breaker",
      "recovered",
    ]);
    expect(latestCurrentCredentialStatus(rows, 101)?.health_kind).toBe("breaker");
  });

  it("counts distinct current unhealthy models", () => {
    const rows = [
      { model_id: "active", health_kind: "auth_dead", health_json: null },
      { model_id: "active", health_kind: "breaker", health_json: null },
      { model_id: "expired", health_kind: "rate_limited", health_json: { open_until: 100 } },
      { model_id: "healthy", health_kind: "recovered", health_json: null },
    ];

    expect(countCurrentUnhealthyModels(rows, 101)).toBe(1);
  });
});

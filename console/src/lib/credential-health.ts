export interface CredentialHealthLike {
  health_kind: string;
  health_json: { open_until?: number } | null;
}

export interface DatedCredentialHealthLike extends CredentialHealthLike {
  updated_at: number;
}

/** Model-scoped row; `updated_at` is optional so callers can pass trimmed rows. */
export interface ModelHealthLike extends CredentialHealthLike {
  model_id: string;
  updated_at?: number;
}

function isExpiredCooldown(status: CredentialHealthLike, nowSecs: number): boolean {
  const until = status.health_json?.open_until;
  return (
    (status.health_kind === "rate_limited" || status.health_kind === "auth_dead") &&
    typeof until === "number" &&
    Number.isFinite(until) &&
    until <= nowSecs
  );
}

export function isCurrentCredentialStatus(status: CredentialHealthLike, nowSecs: number): boolean {
  return !isExpiredCooldown(status, nowSecs);
}

export function countCurrentUnhealthyModels<T extends ModelHealthLike>(
  rows: T[],
  nowSecs: number,
): number {
  return currentUnhealthyModels(rows, nowSecs).length;
}

/** Current non-recovered model rows, one per `model_id` (the most recent wins). */
export function currentUnhealthyModels<T extends ModelHealthLike>(
  rows: T[],
  nowSecs: number,
): T[] {
  const latest = new Map<string, T>();
  for (const row of currentCredentialStatuses(rows, nowSecs)) {
    if (row.health_kind === "recovered") continue;
    const seen = latest.get(row.model_id);
    if (!seen || (seen.updated_at ?? 0) < (row.updated_at ?? 0)) latest.set(row.model_id, row);
  }
  return [...latest.values()].sort((a, b) => a.model_id.localeCompare(b.model_id));
}

export function currentCredentialStatuses<T extends CredentialHealthLike>(
  rows: T[],
  nowSecs: number,
): T[] {
  return rows.filter((row) => isCurrentCredentialStatus(row, nowSecs));
}

export function latestCurrentCredentialStatus<T extends DatedCredentialHealthLike>(
  rows: T[],
  nowSecs: number,
): T | undefined {
  return [...currentCredentialStatuses(rows, nowSecs)].sort((a, b) => b.updated_at - a.updated_at)[0];
}

export function unixNow(): number {
  return Math.floor(Date.now() / 1000);
}

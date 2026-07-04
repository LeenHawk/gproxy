import { queryOptions } from "@tanstack/react-query";
import { api } from "./http";

export interface CredentialView {
  id: number;
  provider_id: number;
  label: string | null;
  kind: string;
  weight: number;
  rpm_limit: number | null;
  tpm_limit: number | null;
  proxy_url: string | null;
  tls_fingerprint: unknown;
  enabled: boolean;
  has_secret: boolean;
}

export interface CredentialUpsert {
  id?: number | null;
  label?: string | null;
  kind: string;
  /** PLAINTEXT — sealed server-side. Required on create; omit on update to keep. */
  secret_json?: unknown;
  weight: number;
  rpm_limit?: number | null;
  tpm_limit?: number | null;
  proxy_url?: string | null;
  /** OMIT when none — sending JSON null becomes Some(Value::Null) server-side
   *  (serde default only applies to absent keys), which reads as "configured". */
  tls_fingerprint?: unknown;
  enabled: boolean;
}

export type HealthKind = "breaker" | "recovered" | "rate_limited" | "auth_dead";

export interface CredentialStatus {
  id: number;
  credential_id: number;
  channel: string;
  health_kind: HealthKind | (string & {});
  health_json: { state?: string; open_until?: number; consecutive_failures?: number; reason?: string } | null;
  checked_at: number | null;
  last_error: string | null;
  created_at: number;
  updated_at: number;
}

export interface UsageWindow {
  name: string;
  label?: string;
  used_percent?: number;
  used?: number;
  limit?: number;
  resets_at?: string;
  resets_at_unix?: number;
  window_seconds?: number;
}

export interface UsageCredits {
  has_credits?: boolean;
  unlimited?: boolean;
  balance?: string;
  used_credits?: number;
  monthly_limit?: number;
  currency?: string;
}

export interface RateLimitResetCredits {
  available_count: number;
}

export interface UsageSnapshot {
  plan?: string;
  windows: UsageWindow[];
  credits?: UsageCredits;
  rate_limit_reset_credits?: RateLimitResetCredits;
  raw: unknown;
}

export type RateLimitResetCreditOutcome = "reset" | "nothing_to_reset" | "no_credit" | "already_redeemed";

export interface RateLimitResetCreditConsumeResponse {
  outcome: RateLimitResetCreditOutcome;
  windows_reset?: number;
  raw: unknown;
}

export const credentialsQuery = (providerId: number) =>
  queryOptions({
    queryKey: ["providers", providerId, "credentials"],
    queryFn: () => api<CredentialView[]>(`/admin/providers/${providerId}/credentials`),
  });

export const credentialStatusQuery = (credentialId: number) =>
  queryOptions({
    queryKey: ["credentials", credentialId, "status"],
    queryFn: () => api<CredentialStatus[]>(`/admin/credentials/${credentialId}/status`),
    staleTime: 30_000,
  });

/** LIVE upstream query — expensive; consumers must keep enabled:false and refetch manually. */
export const credentialUsageQuery = (credentialId: number) =>
  queryOptions({
    queryKey: ["credentials", credentialId, "usage"],
    queryFn: () => api<UsageSnapshot>(`/admin/credentials/${credentialId}/usage`),
    enabled: false,
    retry: false,
    staleTime: Infinity,
  });

export function consumeRateLimitResetCredit(
  credentialId: number,
  idempotencyKey: string,
): Promise<RateLimitResetCreditConsumeResponse> {
  return api<RateLimitResetCreditConsumeResponse>(`/admin/credentials/${credentialId}/rate-limit-reset-credit`, {
    method: "POST",
    body: JSON.stringify({ idempotency_key: idempotencyKey }),
  });
}

export function upsertCredential(providerId: number, input: CredentialUpsert): Promise<CredentialView> {
  return api<CredentialView>(`/admin/providers/${providerId}/credentials`, {
    method: "POST",
    body: JSON.stringify(input),
  });
}

export function deleteCredential(id: number): Promise<void> {
  return api<void>(`/admin/credentials/${id}`, { method: "DELETE" });
}

export function revealSecret(id: number): Promise<unknown> {
  return api<unknown>(`/admin/credentials/${id}/secret`);
}

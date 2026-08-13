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
  provider_id: number | null;
  channel: string;
  health_kind: HealthKind | (string & {});
  health_json: { state?: string; open_until?: number; consecutive_failures?: number; reason?: string } | null;
  checked_at: number | null;
  last_error: string | null;
  created_at: number;
  updated_at: number;
}

export interface CredentialModelStatus extends CredentialStatus {
  model_id: string;
}

export interface UsageTokenTotals {
  requests: number;
  input_tokens: number;
  output_tokens: number;
  image_output_tokens: number;
  cache_read_tokens: number;
  cache_creation_tokens: number;
  total_tokens: number;
  cost_usd: string;
}

export interface UsageModelTotals extends UsageTokenTotals {
  model: string;
}

export interface UsageWindowLocalUsage {
  period_start?: number;
  observed_at: number;
  coverage: "complete" | "partial" | "unknown";
  scope: "all" | "models" | "feature" | "unknown";
  totals: UsageTokenTotals;
  by_model: UsageModelTotals[];
  estimated_capacity?: {
    tokens?: number;
    cost_usd?: string;
    basis: "current_mix";
  };
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
  local_usage?: UsageWindowLocalUsage;
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

export interface CredentialUsageDay {
  day_start: number;
  totals: UsageTokenTotals;
}

/** Local, persisted usage accounting. This endpoint never contacts the upstream. */
export interface CredentialUsageSummary {
  coverage_start?: number;
  lifetime: UsageTokenTotals;
  last_7_days: CredentialUsageDay[];
  by_model: UsageModelTotals[];
}

export interface CredentialQuotaCycle {
  id: number;
  credential_id: number;
  provider_id: number;
  channel: string;
  window_key: string;
  name: string;
  label: string | null;
  scope_kind: string;
  scope_json: unknown | null;
  meter_kind: string;
  period_start: number | null;
  period_end: number | null;
  boundary_source: string;
  boundary_confidence: string;
  close_reason: string | null;
  status: string;
  last_observed_at: number | null;
  used_percent: string | null;
  upstream_used: string | null;
  upstream_limit: string | null;
  coverage: string;
  requests: number;
  input_tokens: number;
  output_tokens: number;
  image_output_tokens: number;
  cache_read_tokens: number;
  cache_creation_5m_tokens: number;
  cache_creation_30m_tokens: number;
  cache_creation_1h_tokens: number;
  cost: string;
  estimated_tokens: number | null;
  estimated_cost: string | null;
  aggregated_through: number | null;
  finalized_at: number | null;
  created_at: number;
  updated_at: number;
}

export interface CredentialQuotaCycleModel {
  id: number;
  cycle_id: number;
  model: string;
  requests: number;
  input_tokens: number;
  output_tokens: number;
  image_output_tokens: number;
  cache_read_tokens: number;
  cache_creation_5m_tokens: number;
  cache_creation_30m_tokens: number;
  cache_creation_1h_tokens: number;
  cost: string;
  created_at: number;
  updated_at: number;
}

export interface CredentialQuotaCycleDetail {
  cycle: CredentialQuotaCycle;
  by_model: CredentialQuotaCycleModel[];
}

export interface CredentialQuotaCycleFilter {
  credential_id?: number;
  provider_id?: number;
  channel?: string;
  window_key?: string;
  status?: string;
  from?: number;
  to?: number;
  before_id?: number;
  limit?: number;
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

export const credentialModelStatusesQuery = (credentialId: number) =>
  queryOptions({
    queryKey: ["credentials", credentialId, "model-statuses"],
    queryFn: () =>
      api<CredentialModelStatus[]>(`/admin/credentials/${credentialId}/model-statuses`),
    staleTime: 30_000,
  });

/** Operator reset: drops the credential-wide health snapshot and the serving
 *  instance's breaker/cooldown. Health is per-instance soft state. */
export function clearCredentialStatus(credentialId: number): Promise<void> {
  return api<void>(`/admin/credentials/${credentialId}/status`, { method: "DELETE" });
}

/** Same reset, scoped to the credential's model-bound health entries. */
export function clearCredentialModelStatuses(credentialId: number): Promise<void> {
  return api<void>(`/admin/credentials/${credentialId}/model-statuses`, { method: "DELETE" });
}

/** LIVE upstream query — expensive; consumers must keep enabled:false and refetch manually. */
export const credentialUsageQuery = (credentialId: number) =>
  queryOptions({
    queryKey: ["credentials", credentialId, "usage"],
    queryFn: () => api<UsageSnapshot>(`/admin/credentials/${credentialId}/usage`),
    enabled: false,
    retry: false,
    staleTime: Infinity,
  });

/** Local-only summary for every credential kind, including plain API keys. */
export const credentialUsageSummaryQuery = (credentialId: number) =>
  queryOptions({
    queryKey: ["credentials", credentialId, "usage-summary"],
    queryFn: () =>
      api<CredentialUsageSummary>(`/admin/credentials/${credentialId}/usage-summary`),
    staleTime: 30_000,
  });

function credentialQuotaCycleQueryString(filter: CredentialQuotaCycleFilter): string {
  const params = new URLSearchParams();
  if (filter.credential_id !== undefined) params.set("credential_id", String(filter.credential_id));
  if (filter.provider_id !== undefined) params.set("provider_id", String(filter.provider_id));
  if (filter.channel) params.set("channel", filter.channel);
  if (filter.window_key) params.set("window_key", filter.window_key);
  if (filter.status) params.set("status", filter.status);
  if (filter.from !== undefined) params.set("from", String(filter.from));
  if (filter.to !== undefined) params.set("to", String(filter.to));
  if (filter.before_id !== undefined) params.set("before_id", String(filter.before_id));
  if (filter.limit !== undefined) params.set("limit", String(filter.limit));
  const query = params.toString();
  return query ? `?${query}` : "";
}

export function fetchCredentialQuotaCycles(
  filter: CredentialQuotaCycleFilter = {},
): Promise<CredentialQuotaCycle[]> {
  return api<CredentialQuotaCycle[]>(
    `/admin/credential-quota-cycles${credentialQuotaCycleQueryString(filter)}`,
  );
}

export const credentialQuotaCyclesQuery = (filter: CredentialQuotaCycleFilter = {}) =>
  queryOptions({
    queryKey: ["credential-quota-cycles", filter],
    queryFn: () => fetchCredentialQuotaCycles(filter),
    staleTime: 30_000,
  });

export const credentialQuotaCycleDetailQuery = (cycleId: number) =>
  queryOptions({
    queryKey: ["credential-quota-cycles", cycleId],
    queryFn: () => api<CredentialQuotaCycleDetail>(`/admin/credential-quota-cycles/${cycleId}`),
    staleTime: 30_000,
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

export interface CredentialImportItemResult {
  index: number;
  status: "created" | "existing" | "error";
  id?: number;
  error?: string;
}

export interface CredentialImportOutcome {
  created: number;
  existing: number;
  failed: number;
  results: CredentialImportItemResult[];
}

/** Batch create-only import — one request; per-item results in input order. */
export function importCredentials(
  providerId: number,
  items: CredentialUpsert[],
): Promise<CredentialImportOutcome> {
  return api<CredentialImportOutcome>(`/admin/providers/${providerId}/credentials/import`, {
    method: "POST",
    body: JSON.stringify({ items }),
  });
}

export function deleteCredential(id: number): Promise<void> {
  return api<void>(`/admin/credentials/${id}`, { method: "DELETE" });
}

export function revealSecret(id: number): Promise<unknown> {
  return api<unknown>(`/admin/credentials/${id}/secret`);
}

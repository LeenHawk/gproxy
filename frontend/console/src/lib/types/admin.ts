import type { Scope, UsageQuery, UsageQueryRow } from "./shared";

export type ReloadResponse = {
  ok: true;
  providers: number;
  users: number;
  keys: number;
  models: number;
  user_files: number;
  claude_files: number;
  aliases: number;
  permissions: number;
  file_permissions: number;
  rate_limits: number;
  quotas: number;
};

export type GlobalSettings = {
  host: string;
  port: number;
  proxy?: string | null;
  spoof_emulation: string;
  enable_usage: boolean;
  enable_upstream_log: boolean;
  enable_upstream_log_body: boolean;
  enable_downstream_log: boolean;
  enable_downstream_log_body: boolean;
  dsn: string;
  data_dir: string;
};

export type ProviderQueryParams = {
  name?: Scope<string>;
  channel?: Scope<string>;
};

export type DispatchRoute = {
  operation: string;
  protocol: string;
};

export type DispatchImplementation =
  | "Passthrough"
  | "Local"
  | "Unsupported"
  | {
      TransformTo: {
        destination: DispatchRoute;
      };
    };

export type DispatchRuleDocument = {
  route: DispatchRoute;
  implementation: DispatchImplementation;
};

export type DispatchTableDocument = {
  rules: DispatchRuleDocument[];
};

export type ProviderRow = {
  id: number;
  name: string;
  channel: string;
  label?: string | null;
  settings_json: Record<string, unknown>;
  dispatch_json: DispatchTableDocument | null;
  credential_count: number;
};

export type ProviderWrite = {
  id: number;
  name: string;
  channel: string;
  label?: string | null;
  settings_json: string;
  dispatch_json: string;
};

export type ProviderDispatchTemplateParams = {
  channel: string;
};

export type DeleteProviderPayload = {
  name: string;
};

export type CredentialQueryParams = {
  provider_name?: Scope<string>;
};

export type CredentialRow = {
  id: number;
  provider: string;
  index: number;
  credential: Record<string, unknown>;
};

export type UpsertCredentialPayload = {
  provider_name: string;
  credential: Record<string, unknown>;
};

export type DeleteCredentialPayload = {
  provider_name: string;
  index: number;
};

export type CredentialHealthQuery = {
  provider_name?: Scope<string>;
};

export type CredentialHealthRow = {
  provider: string;
  index: number;
  status: string;
  available: boolean;
};

export type UpdateCredentialStatusPayload = {
  provider_name: string;
  index: number;
  status: "healthy" | "dead";
};

export type ModelQueryParams = {
  id?: Scope<number>;
  provider_id?: Scope<number>;
  model_id?: Scope<string>;
  enabled?: Scope<boolean>;
  alias_of_filter?: "only_aliases" | "only_real";
  limit?: number;
  offset?: number;
};

export type MemoryModelRow = {
  id: number;
  provider_id: number;
  model_id: string;
  display_name?: string | null;
  enabled: boolean;
  /// Full serialized ModelPrice JSON blob (same shape as `models.pricing_json`).
  /// Covers every billing mode (default / flex / scale / priority).
  /// `null` on rows with no pricing configured.
  pricing_json?: string | null;
  alias_of?: number | null;
};

export type ModelWrite = {
  id: number;
  provider_id: number;
  model_id: string;
  display_name?: string | null;
  enabled: boolean;
  /// Legacy column, left nullable for schema compatibility only. The admin
  /// API no longer reads it — use `pricing_json` instead.
  price_each_call?: number | null;
  /// Legacy column, left nullable for schema compatibility only. The admin
  /// API no longer reads it — use `pricing_json` instead.
  price_tiers_json?: string | null;
  /// Authoritative serialized ModelPrice blob. Must be valid JSON matching
  /// the `ModelPrice` struct shape in `sdk/gproxy-provider/src/billing.rs`.
  pricing_json?: string | null;
  alias_of?: number | null;
};

export type UserQueryParams = {
  id?: Scope<number>;
  name?: Scope<string>;
};

export type MemoryUserRow = {
  id: number;
  name: string;
  enabled: boolean;
  is_admin: boolean;
};

export type UserWrite = {
  id: number;
  name: string;
  password: string;
  enabled: boolean;
  is_admin: boolean;
};

export type UserKeyQueryParams = {
  user_id?: Scope<number>;
};

export type MemoryUserKeyRow = {
  id: number;
  user_id: number;
  api_key: string;
  label?: string | null;
  enabled: boolean;
};

export type GenerateUserKeyPayload = {
  user_id: number;
  label?: string | null;
};

export type GenerateUserKeyResponse = {
  ok: true;
  id: number;
  api_key: string;
};

export type BatchGenerateUserKeysPayload = {
  user_id: number;
  count: number;
  label?: string | null;
};

export type BatchGenerateUserKeysResponse = {
  ok: true;
  keys: Array<{
    id: number;
    api_key: string;
  }>;
};

export type PermissionQueryParams = {
  user_id?: Scope<number>;
  provider_id?: Scope<number>;
  limit?: number;
};

export type MemoryPermissionRow = {
  id: number;
  user_id: number;
  provider_id?: number | null;
  model_pattern: string;
};

export type UserModelPermissionWrite = {
  id: number;
  user_id: number;
  provider_id?: number | null;
  model_pattern: string;
};

export type FilePermissionQueryParams = {
  user_id?: Scope<number>;
  provider_id?: Scope<number>;
  limit?: number;
};

export type MemoryFilePermissionRow = {
  id: number;
  user_id: number;
  provider_id: number;
};

export type UserFilePermissionWrite = {
  id: number;
  user_id: number;
  provider_id: number;
};

export type RateLimitQueryParams = {
  user_id?: Scope<number>;
  limit?: number;
};

export type MemoryRateLimitRow = {
  id: number;
  user_id: number;
  model_pattern: string;
  rpm?: number | null;
  rpd?: number | null;
  total_tokens?: number | null;
};

export type UserRateLimitWrite = {
  id: number;
  user_id: number;
  model_pattern: string;
  rpm?: number | null;
  rpd?: number | null;
  total_tokens?: number | null;
};

export type MemoryUserQuotaRow = {
  user_id: number;
  quota: number;
  cost_used: number;
  remaining: number;
};

export type UserQuotaWrite = {
  user_id: number;
  quota: number;
  cost_used: number;
};

export type UpstreamRequestQuery = {
  trace_id?: Scope<number>;
  provider_id?: Scope<number>;
  credential_id?: Scope<number>;
  request_url_contains?: string;
  from_unix_ms?: number;
  to_unix_ms?: number;
  cursor_at_unix_ms?: number;
  cursor_trace_id?: number;
  offset?: number;
  limit?: number;
  include_body?: boolean;
};

export type UpstreamRequestQueryRow = {
  trace_id: number;
  downstream_trace_id?: number | null;
  at: string;
  internal: boolean;
  provider_id?: number | null;
  credential_id?: number | null;
  request_method: string;
  request_headers_json: Record<string, unknown>;
  request_url?: string | null;
  request_body?: number[] | null;
  response_status?: number | null;
  response_headers_json: Record<string, unknown>;
  response_body?: number[] | null;
  initial_latency_ms?: number | null;
  total_latency_ms?: number | null;
  created_at: string;
};

export type DownstreamRequestQuery = {
  trace_id?: Scope<number>;
  user_id?: Scope<number>;
  user_key_id?: Scope<number>;
  request_path_contains?: string;
  from_unix_ms?: number;
  to_unix_ms?: number;
  cursor_at_unix_ms?: number;
  cursor_trace_id?: number;
  offset?: number;
  limit?: number;
  include_body?: boolean;
};

export type DownstreamRequestQueryRow = {
  trace_id: number;
  at: string;
  internal: boolean;
  user_id?: number | null;
  user_key_id?: number | null;
  request_method: string;
  request_headers_json: Record<string, unknown>;
  request_path: string;
  request_query?: string | null;
  request_body?: number[] | null;
  response_status?: number | null;
  response_headers_json: Record<string, unknown>;
  response_body?: number[] | null;
  created_at: string;
};

export type DashboardQuery = {
  from_unix_ms: number;
  to_unix_ms: number;
  bucket_seconds: number;
};

export type DashboardKpi = {
  total_requests: number;
  success_count: number;
  error_4xx_count: number;
  error_5xx_count: number;
  total_cost: number;
  total_input_tokens: number;
  total_output_tokens: number;
  avg_latency_ms?: number | null;
  max_latency_ms?: number | null;
};

export type DashboardTrafficBucket = {
  bucket: number;
  request_count: number;
  cost: number;
};

export type DashboardStatusBucket = {
  bucket: number;
  ok: number;
  err_4xx: number;
  err_5xx: number;
};

export type DashboardOverview = {
  kpi: DashboardKpi;
  traffic: DashboardTrafficBucket[];
  status_codes: DashboardStatusBucket[];
};

export type DashboardTopProviderRow = {
  provider_id?: number | null;
  channel?: string | null;
  request_count: number;
  total_cost: number;
  total_input_tokens: number;
  total_output_tokens: number;
};

export type DashboardTopProviders = {
  rows: DashboardTopProviderRow[];
};

export type DashboardTopModelRow = {
  model?: string | null;
  request_count: number;
  total_cost: number;
  total_input_tokens: number;
  total_output_tokens: number;
};

export type DashboardTopModels = {
  rows: DashboardTopModelRow[];
};

/// Payload for `POST /admin/requests/{upstream,downstream}/clear`.
/// `all: true` clears every row under the current filter set regardless of
/// `trace_ids`. `all: false` requires a non-empty `trace_ids` list and only
/// clears those rows. Clearing wipes `request_headers_json` /
/// `request_body` / `response_headers_json` / `response_body` but keeps
/// the log row itself (unlike `batch-delete` which removes the row).
export type ClearRequestPayload = {
  all: boolean;
  trace_ids: number[];
};

export type RequestClearAck = {
  ok: boolean;
  cleared: number;
};

export type UpdateCheckResponse = {
  current_version: string;
  latest_version?: string | null;
  update_available: boolean;
  download_url?: string | null;
};

export type UpdateParams = {
  tag?: string | null;
};

export type UpdatePerformResponse = {
  ok: boolean;
  old_version: string;
  new_version: string;
  message: string;
};

export type OAuthStartResponse = {
  authorize_url: string;
  state: string;
  redirect_uri?: string | null;
  verification_uri?: string | null;
  user_code?: string | null;
  mode?: string | null;
  scope?: string | null;
  instructions?: string | null;
};

export type OAuthCallbackResponse = {
  credential: Record<string, unknown>;
  details: Record<string, unknown>;
};

export type {
  UsageQuery,
  UsageQueryRow,
};

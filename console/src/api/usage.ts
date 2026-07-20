import { keepPreviousData, queryOptions } from "@tanstack/react-query";
import { PAGE_SIZE, type PageResult } from "./pagination";
import { api } from "./http";

export interface Usage {
  id: number;
  request_id: string;
  at: number;
  route_name: string | null;
  provider_id: number | null;
  credential_id: number | null;
  org_id: number | null;
  team_id: number | null;
  user_id: number | null;
  user_key_id: number | null;
  operation: string;
  kind: string;
  model: string | null;
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_creation_5m_tokens: number;
  cache_creation_30m_tokens: number;
  cache_creation_1h_tokens: number;
  cost: string;
  latency_ms: number;
  usage_source: string;
  ended: string;
}

export interface UsageRollup {
  id: number;
  granularity: string;
  bucket_start: number;
  provider_id: number | null;
  org_id: number | null;
  team_id: number | null;
  user_id: number | null;
  route_name: string | null;
  model: string | null;
  requests: number;
  input_tokens: number;
  output_tokens: number;
  cache_write_tokens: number;
  cache_read_tokens: number;
  cost: string;
}

export interface UsageSummary {
  requests: number;
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_creation_5m_tokens: number;
  cache_creation_30m_tokens: number;
  cache_creation_1h_tokens: number;
  cost: string;
}

export interface DownstreamRequest {
  id: number;
  request_id: string;
  at: number;
  method: string;
  path: string;
  query: string | null;
  status: number;
  headers_json: unknown;
  body: string | null;
  response_body: string | null;
}

export interface UpstreamRequest {
  id: number;
  request_id: string;
  at: number;
  provider_id: number | null;
  credential_id: number | null;
  url: string;
  method: string;
  status: number;
  latency_ms: number;
  headers_json: unknown;
  body: string | null;
  response_body: string | null;
}

export interface AuditLog {
  id: number;
  at: number;
  actor_id: number | null;
  actor_name: string | null;
  action: string;
  target: string;
  status: number;
  source_ip: string | null;
}

export interface CredentialStatus {
  id: number;
  credential_id: number;
  channel: string;
  health_kind: string;
  health_json: { state?: string; open_until?: number; reason?: string } | null;
  checked_at: number | null;
  last_error: string | null;
  created_at: number;
  updated_at: number;
}

export interface CredentialModelStatus extends CredentialStatus {
  model_id: string;
}

export interface UsageFilter {
  at_from?: number;
  at_to?: number;
  provider_id?: number;
  user_id?: number;
  route_name?: string;
  model?: string;
  before_id?: number;
  limit?: number;
}

export interface AuditFilter {
  at_from?: number;
  at_to?: number;
  actor_id?: number;
  action?: string;
  target?: string;
  status?: number;
  source_ip?: string;
}

function usageQs(f: UsageFilter, page?: number): string {
  const p = new URLSearchParams();
  if (f.limit != null) p.set("limit", String(f.limit));
  if (f.before_id != null) p.set("before_id", String(f.before_id));
  if (f.at_from != null) p.set("at_from", String(f.at_from));
  if (f.at_to != null) p.set("at_to", String(f.at_to));
  if (f.provider_id != null) p.set("provider_id", String(f.provider_id));
  if (f.user_id != null) p.set("user_id", String(f.user_id));
  if (f.route_name) p.set("route_name", f.route_name);
  if (f.model) p.set("model", f.model);
  if (page != null) {
    p.set("page", String(page));
    p.set("page_size", String(PAGE_SIZE));
  }
  const s = p.toString();
  return s ? `?${s}` : "";
}

function auditQs(f: AuditFilter, page: number): string {
  const p = new URLSearchParams();
  if (f.at_from != null) p.set("at_from", String(f.at_from));
  if (f.at_to != null) p.set("at_to", String(f.at_to));
  if (f.actor_id != null) p.set("actor_id", String(f.actor_id));
  if (f.action) p.set("action", f.action);
  if (f.target) p.set("target", f.target);
  if (f.status != null) p.set("status", String(f.status));
  if (f.source_ip) p.set("source_ip", f.source_ip);
  p.set("page", String(page));
  p.set("page_size", String(PAGE_SIZE));
  return `?${p.toString()}`;
}

export const usageQuery = (f: UsageFilter) =>
  queryOptions({
    queryKey: ["usage", f],
    queryFn: () => api<Usage[]>(`/admin/usage${usageQs(f)}`),
  });

export const usagePageQuery = (f: Omit<UsageFilter, "before_id" | "limit">, page: number) =>
  queryOptions({
    queryKey: ["usage", "page", f, page],
    queryFn: () => api<PageResult<Usage>>(`/admin/usage${usageQs(f, page)}`),
    placeholderData: keepPreviousData,
  });

export const usageSummaryQuery = (
  f: Omit<UsageFilter, "before_id" | "limit">,
) =>
  queryOptions({
    queryKey: ["usage", "summary", f],
    queryFn: () =>
      api<UsageSummary>(`/admin/usage-summary${usageQs(f)}`),
  });

export const logsPageQuery = (
  f: Omit<UsageFilter, "before_id" | "limit" | "model">,
  page: number,
) => queryOptions({
  queryKey: ["logs", "page", f, page],
  queryFn: () => api<PageResult<DownstreamRequest>>(`/admin/logs${usageQs(f, page)}`),
  placeholderData: keepPreviousData,
});

export const rollupsQuery = (granularity: string, from: number, to: number) =>
  queryOptions({
    queryKey: ["usage-rollups", granularity, from, to],
    queryFn: () =>
      api<UsageRollup[]>(
        `/admin/usage-rollups?granularity=${granularity}&from=${from}&to=${to}`,
      ),
  });

export const downstreamLogsQuery = (rid: string) =>
  queryOptions({
    queryKey: ["logs", rid, "downstream"],
    queryFn: () => api<DownstreamRequest[]>(`/admin/logs/${rid}/downstream`),
  });

export const upstreamLogsQuery = (rid: string) =>
  queryOptions({
    queryKey: ["logs", rid, "upstream"],
    queryFn: () => api<UpstreamRequest[]>(`/admin/logs/${rid}/upstream`),
  });

export const auditPageQuery = (f: AuditFilter, page: number) =>
  queryOptions({
    queryKey: ["audit", "page", f, page],
    queryFn: () => api<PageResult<AuditLog>>(`/admin/audit${auditQs(f, page)}`),
    placeholderData: keepPreviousData,
  });

export const credentialStatusesQuery = queryOptions({
  queryKey: ["credential-statuses"],
  queryFn: () => api<CredentialStatus[]>("/admin/credential-statuses"),
  staleTime: 30_000,
});

export const credentialModelStatusesQuery = queryOptions({
  queryKey: ["credential-model-statuses"],
  queryFn: () => api<CredentialModelStatus[]>("/admin/credential-model-statuses"),
  staleTime: 30_000,
});

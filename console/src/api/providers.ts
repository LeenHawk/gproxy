import { queryOptions } from "@tanstack/react-query";
import { api } from "./http";

export interface TlsPreset {
  id: string;
  label: string;
  fingerprint: unknown;
}

export interface Provider {
  id: number;
  name: string;
  channel: string;
  label: string | null;
  settings_json: unknown;
  credential_strategy: string;
  proxy_url: string | null;
  tls_fingerprint: unknown;
  enabled: boolean;
  created_at: number;
  updated_at: number;
}

export interface ProviderInput {
  id?: number | null;
  name: string;
  channel: string;
  label?: string | null;
  settings_json: unknown;
  credential_strategy: string;
  proxy_url?: string | null;
  /** OMIT when none — sending JSON null becomes Some(Value::Null) server-side
   *  (serde default only applies to absent keys), which reads as "configured". */
  tls_fingerprint?: unknown;
  enabled: boolean;
}

export const providersQuery = queryOptions({
  queryKey: ["providers"],
  queryFn: () => api<Provider[]>("/admin/providers"),
});

export const tlsPresetsQuery = queryOptions({
  queryKey: ["tls-presets"],
  queryFn: () => api<TlsPreset[]>("/admin/tls-presets"),
  staleTime: 1000 * 60 * 60, // 1 hour — static list
});

export const providerQuery = (id: number) =>
  queryOptions({
    queryKey: ["providers", id],
    queryFn: () => api<Provider>(`/admin/providers/${id}`),
  });

export function upsertProvider(input: ProviderInput): Promise<Provider> {
  return api<Provider>("/admin/providers", { method: "POST", body: JSON.stringify(input) });
}

export function deleteProvider(id: number): Promise<void> {
  return api<void>(`/admin/providers/${id}`, { method: "DELETE" });
}

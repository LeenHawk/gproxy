import { queryOptions } from "@tanstack/react-query";
import { api } from "./http";

export interface ProviderModel {
  id: number;
  provider_id: number;
  model_id: string;
  display_name: string | null;
  variants_json: unknown;
  enabled: boolean;
  created_at: number;
  updated_at: number;
}

export interface ProviderModelInput {
  id?: number | null;
  provider_id: number;
  model_id: string;
  display_name?: string | null;
  /** OMIT when none — variants_json HAS serde(default); sending JSON null round-trips as
   *  Some(Value::Null), so omit the key rather than send null. */
  variants_json?: unknown;
  enabled: boolean;
}

export const providerModelsQuery = (providerId: number) =>
  queryOptions({
    queryKey: ["providers", providerId, "models"],
    queryFn: () => api<ProviderModel[]>(`/admin/providers/${providerId}/models`),
  });

export function upsertProviderModel(providerId: number, input: ProviderModelInput): Promise<ProviderModel> {
  return api<ProviderModel>(`/admin/providers/${providerId}/models`, {
    method: "POST",
    body: JSON.stringify(input),
  });
}

export function deleteProviderModel(id: number): Promise<void> {
  return api<void>(`/admin/provider-models/${id}`, { method: "DELETE" });
}

export interface UpstreamModel { id: string; display_name: string | null; }

/** LIVE upstream pull — keep enabled:false and refetch manually (it calls the provider's API). */
export const upstreamModelsQuery = (providerId: number) =>
  queryOptions({
    queryKey: ["providers", providerId, "upstream-models"],
    queryFn: () => api<UpstreamModel[]>(`/admin/providers/${providerId}/upstream-models`),
    enabled: false,
    retry: false,
    staleTime: 0,
    gcTime: 0,
  });

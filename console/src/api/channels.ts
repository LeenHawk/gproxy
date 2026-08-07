import { queryOptions } from "@tanstack/react-query";
import { api, ApiError } from "./http";

export interface ChannelSettingFieldDto {
  key: string;
  control: "text" | "url" | "boolean" | "integer" | "string_list";
  label?: string;
  required?: boolean;
  default?: unknown;
  placeholder?: string;
}

export interface ChannelCatalogDto {
  source: "builtin" | "external";
  id: string;
  display_name: string;
  credential_family: "api_key" | "oauth_tokens" | "service_account" | "github_token";
  login_modes: ("authcode" | "device" | "cookie")[];
  settings_fields: ChannelSettingFieldDto[];
  secret_template: unknown;
  endpoint_kinds: string[];
  usage: boolean;
}

export function isLegacyChannelCatalogError(error: unknown): boolean {
  return error instanceof ApiError && (error.status === 404 || error.status === 405);
}

export const channelsQuery = queryOptions({
  queryKey: ["channels"],
  queryFn: () => api<ChannelCatalogDto[]>("/admin/channels"),
  staleTime: 60_000,
  refetchOnMount: true,
  refetchOnWindowFocus: true,
  retry: (failureCount, error) => !isLegacyChannelCatalogError(error) && failureCount < 1,
});

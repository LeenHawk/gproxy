import { queryOptions } from "@tanstack/react-query";
import { api } from "./http";

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
  provider_family: "open_ai" | "claude" | "gemini";
  credential_family: "api_key" | "oauth_tokens" | "service_account" | "github_token";
  login_modes: ("authcode" | "device" | "cookie")[];
  settings_fields: ChannelSettingFieldDto[];
  secret_template: unknown;
  endpoint_kinds: string[];
  usage: boolean;
}

export const channelsQuery = queryOptions({
  queryKey: ["channels"],
  queryFn: () => api<ChannelCatalogDto[]>("/admin/channels"),
  staleTime: Infinity,
});

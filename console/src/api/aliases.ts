import { queryOptions } from "@tanstack/react-query";
import { api } from "./http";

export interface Alias {
  id: number;
  alias: string;
  route_id: number;
  created_at: number;
  updated_at: number;
}

export interface AliasInput {
  id?: number | null;
  alias: string;
  route_id: number;
}

export const aliasesQuery = queryOptions({
  queryKey: ["aliases"],
  queryFn: () => api<Alias[]>("/admin/aliases"),
});

export function upsertAlias(input: AliasInput): Promise<Alias> {
  return api<Alias>("/admin/aliases", { method: "POST", body: JSON.stringify(input) });
}

export function deleteAlias(id: number): Promise<void> {
  return api<void>(`/admin/aliases/${id}`, { method: "DELETE" });
}

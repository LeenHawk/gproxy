import { queryOptions } from "@tanstack/react-query";
import { api } from "./http";

export interface Org { id: number; name: string; enabled: boolean; description: string | null; created_at: number; updated_at: number; }
export interface OrgInput { id?: number | null; name: string; enabled: boolean; description?: string | null; }
export interface Team { id: number; org_id: number; name: string; enabled: boolean; created_at: number; updated_at: number; }
export interface TeamInput { id?: number | null; org_id: number; name: string; enabled: boolean; }
export interface UserView { id: number; name: string; org_id: number; team_id: number | null; has_password: boolean; enabled: boolean; is_admin: boolean; created_at: number; updated_at: number; }
export interface UserUpsert { id?: number | null; name: string; org_id: number; team_id?: number | null; password?: string; enabled: boolean; is_admin: boolean; }
export interface UserKeyView { id: number; user_id: number; label: string | null; enabled: boolean; key_prefix: string; api_key?: string; }
export interface UserKeyUpsert { id?: number | null; label?: string | null; enabled: boolean; }

export const orgsQuery = queryOptions({ queryKey: ["orgs"], queryFn: () => api<Org[]>("/admin/orgs") });
export const orgQuery = (id: number) => queryOptions({ queryKey: ["orgs", id], queryFn: () => api<Org>(`/admin/orgs/${id}`) });
export const teamsQuery = (orgId: number) => queryOptions({ queryKey: ["orgs", orgId, "teams"], queryFn: () => api<Team[]>(`/admin/orgs/${orgId}/teams`) });
export const usersQuery = queryOptions({ queryKey: ["users"], queryFn: () => api<UserView[]>("/admin/users") });
export const userQuery = (id: number) => queryOptions({ queryKey: ["users", id], queryFn: () => api<UserView>(`/admin/users/${id}`) });
export const userKeysQuery = (userId: number) => queryOptions({ queryKey: ["users", userId, "keys"], queryFn: () => api<UserKeyView[]>(`/admin/users/${userId}/keys`) });

export const upsertOrg = (i: OrgInput) => api<Org>("/admin/orgs", { method: "POST", body: JSON.stringify(i) });
export const deleteOrg = (id: number) => api<void>(`/admin/orgs/${id}`, { method: "DELETE" });
export const upsertTeam = (orgId: number, i: TeamInput) => api<Team>(`/admin/orgs/${orgId}/teams`, { method: "POST", body: JSON.stringify(i) });
export const deleteTeam = (id: number) => api<void>(`/admin/teams/${id}`, { method: "DELETE" });
export const upsertUser = (i: UserUpsert) => api<UserView>("/admin/users", { method: "POST", body: JSON.stringify(i) });
export const deleteUser = (id: number) => api<void>(`/admin/users/${id}`, { method: "DELETE" });
export const createUserKey = (userId: number, i: UserKeyUpsert) => api<UserKeyView>(`/admin/users/${userId}/keys`, { method: "POST", body: JSON.stringify(i) });
export const deleteUserKey = (id: number) => api<void>(`/admin/user-keys/${id}`, { method: "DELETE" });

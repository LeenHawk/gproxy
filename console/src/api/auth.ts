import { queryOptions } from "@tanstack/react-query";
import { api } from "./http";

export interface MeResponse {
  id: number;
  name: string;
  is_admin: boolean;
}

export interface LoginResponse {
  user: MeResponse;
}

export const sessionQuery = queryOptions({
  queryKey: ["session"],
  queryFn: () => api<MeResponse>("/admin/me"),
  retry: false,
  staleTime: 60_000,
});

export function login(username: string, password: string): Promise<LoginResponse> {
  return api<LoginResponse>("/admin/login", {
    method: "POST",
    body: JSON.stringify({ username, password }),
  });
}

export function logout(): Promise<void> {
  return api<void>("/admin/logout", { method: "POST" });
}

import { queryOptions } from "@tanstack/react-query";
import { api } from "./http";

export interface CheckReport {
  current: string;
  latest: string;
  available: boolean;
  notes_url: string | null;
}

export type UpdateStatus =
  | { state: "idle" }
  | { state: "checking" }
  | { state: "downloading" }
  | { state: "staged"; version: string }
  | { state: "restarting"; version: string }
  | { state: "failed"; error: string };

// check is a manual, possibly-slow read — disabled by default; refetch on click.
export const updateCheckQuery = queryOptions({
  queryKey: ["update", "check"],
  queryFn: () => api<CheckReport>("/admin/update/check"),
  enabled: false,
  staleTime: 0,
  retry: false,
});

export const updateStatusQuery = queryOptions({
  queryKey: ["update", "status"],
  queryFn: () => api<UpdateStatus>("/admin/update/status"),
});

export function applyUpdate(): Promise<UpdateStatus> {
  return api<UpdateStatus>("/admin/update/apply", { method: "POST", body: "{}" });
}

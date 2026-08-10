import { queryOptions } from "@tanstack/react-query";
import { api } from "./http";

export interface CheckReport {
  current: string;
  /** Target triple of the running binary (arch-vendor-os-libc). Shown next to
   *  the current identity because UPX-packed release artifacts make every Linux
   *  build look identical to `file`/`ldd`. */
  target: string;
  latest: string;
  available: boolean;
  release_notes_available: boolean;
  safety?: UpdateSafetyRisk[];
  install_mode: "binary" | "android_apk";
}

export interface ReleaseNotesEntry {
  version: string;
  body: string;
}

export interface ReleaseNotesReport {
  current: string;
  latest: string;
  complete: boolean;
  entries: ReleaseNotesEntry[];
}

export type UpdateSafetyRisk = "missing_sha256" | "missing_signature" | "missing_public_key";

export type UpdateStatus =
  | { state: "idle" }
  | { state: "checking" }
  | { state: "downloading" }
  | { state: "staged"; version: string }
  | { state: "restarting"; version: string }
  | { state: "failed"; error: string };

// Disabled by default for the manual Updates page. The global admin banner
// explicitly enables this query once when Console opens.
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

export function releaseNotesQuery(current: string, latest: string) {
  return queryOptions({
    queryKey: ["update", "notes", current, latest],
    queryFn: () => api<ReleaseNotesReport>("/admin/update/notes"),
    retry: false,
  });
}

export function applyUpdate(options: { allow_insecure?: boolean } = {}): Promise<UpdateStatus> {
  return api<UpdateStatus>("/admin/update/apply", {
    method: "POST",
    body: JSON.stringify({ allow_insecure: options.allow_insecure === true }),
  });
}

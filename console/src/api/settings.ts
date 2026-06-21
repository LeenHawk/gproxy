import { queryOptions } from "@tanstack/react-query";
import { api } from "./http";

export interface InstanceSettings {
  id: number;
  instance_name: string;
  proxy: string | null;
  spoof_emulation: boolean | null;
  enable_usage: boolean;
  enable_upstream_log: boolean;
  enable_upstream_log_body: boolean;
  enable_downstream_log: boolean;
  enable_downstream_log_body: boolean;
  disable_log_redaction: boolean;
  enable_tokenizer_download: boolean;
  update_channel: string | null;
  retention_days: number | null;
  created_at: number;
  updated_at: number;
}
// Upsert: id:null = create. Send ALL non-Option fields — they lack serde(default) (422 otherwise).
export interface InstanceSettingsInput {
  id?: number | null;
  instance_name: string;
  proxy?: string | null;
  spoof_emulation?: boolean | null;
  enable_usage: boolean;
  enable_upstream_log: boolean;
  enable_upstream_log_body: boolean;
  enable_downstream_log: boolean;
  enable_downstream_log_body: boolean;
  disable_log_redaction: boolean;
  enable_tokenizer_download: boolean;
  update_channel?: string | null;
  retention_days?: number | null;
}

export const instanceSettingsQuery = queryOptions({
  queryKey: ["instance-settings"],
  queryFn: () => api<InstanceSettings[]>("/admin/instance-settings"),
});
export function upsertInstanceSettings(input: InstanceSettingsInput): Promise<InstanceSettings> {
  return api<InstanceSettings>("/admin/instance-settings", { method: "POST", body: JSON.stringify(input) });
}

/** Map a stored row to its upsert input (all fields), so a single field can be
 *  changed without dropping the others (the API replaces the whole row). */
export function settingsToInput(s: InstanceSettings): InstanceSettingsInput {
  return {
    id: s.id,
    instance_name: s.instance_name,
    proxy: s.proxy,
    spoof_emulation: s.spoof_emulation,
    enable_usage: s.enable_usage,
    enable_upstream_log: s.enable_upstream_log,
    enable_upstream_log_body: s.enable_upstream_log_body,
    enable_downstream_log: s.enable_downstream_log,
    enable_downstream_log_body: s.enable_downstream_log_body,
    disable_log_redaction: s.disable_log_redaction,
    enable_tokenizer_download: s.enable_tokenizer_download,
    update_channel: s.update_channel,
    retention_days: s.retention_days,
  };
}

import { api } from "./http";

export type ConnectivityScope = "global" | "provider" | "credential";

export interface ConnectivityTestInput {
  scope: ConnectivityScope;
  proxy_url: string | null;
  provider_id?: number;
}

export interface ConnectivityTestResult {
  ok: boolean;
  ip: string | null;
  ip_version: 4 | 6 | null;
  colo: string | null;
  location: string | null;
  latency_ms: number;
  proxy_source: "credential" | "provider" | "global" | "startup" | "direct";
  error_code: string | null;
  message: string | null;
}

export function testConnectivity(input: ConnectivityTestInput) {
  return api<ConnectivityTestResult>("/admin/connectivity/test", {
    method: "POST",
    body: JSON.stringify(input),
  });
}

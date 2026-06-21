import { api } from "./http";
import type { CredentialView } from "./credentials";

export interface LoginStartRequest {
  channel: string;
  redirect_uri?: string;
  params?: Record<string, unknown>;
}

export interface LoginStartResponse {
  login_session_id: string;
  authorize_url: string;
}

export interface LoginCompleteRequest {
  login_session_id: string;
  callback_url?: string;
  /** Bare authorization code for code-only flows (no callback URL / state). */
  code?: string;
  provider_id: number;
  name?: string;
}

export interface DeviceStartResponse {
  login_session_id: string;
  user_code: string;
  verification_url: string;
  interval_secs: number;
}

export type DevicePollResponse =
  | { status: "pending" }
  | { status: "ready"; credential: CredentialView };

export function loginFlowStart(req: LoginStartRequest): Promise<LoginStartResponse> {
  return api<LoginStartResponse>("/admin/login-flows/start", { method: "POST", body: JSON.stringify(req) });
}

export function loginFlowComplete(req: LoginCompleteRequest): Promise<CredentialView> {
  return api<CredentialView>("/admin/login-flows/complete", { method: "POST", body: JSON.stringify(req) });
}

export function deviceStart(req: { channel: string; provider_id: number; name?: string; params?: Record<string, unknown> }): Promise<DeviceStartResponse> {
  return api<DeviceStartResponse>("/admin/login-flows/device/start", { method: "POST", body: JSON.stringify(req) });
}

export function devicePoll(login_session_id: string): Promise<DevicePollResponse> {
  return api<DevicePollResponse>("/admin/login-flows/device/poll", {
    method: "POST",
    body: JSON.stringify({ login_session_id }),
  });
}

export function cookieLogin(req: { channel: string; cookie: string; provider_id: number; name?: string }): Promise<CredentialView> {
  return api<CredentialView>("/admin/login-flows/cookie", { method: "POST", body: JSON.stringify(req) });
}

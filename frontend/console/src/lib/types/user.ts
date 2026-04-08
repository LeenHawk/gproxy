import type { UsageQuery, UsageQueryRow } from "./shared";

export type UserKeyRow = {
  id: number;
  api_key: string;
  label?: string | null;
  enabled: boolean;
};

export type GenerateKeyPayload = {
  label?: string | null;
};

export type GenerateKeyResponse = {
  ok: true;
  api_key: string;
};

export type QuotaResponse = {
  user_id: number;
  quota: number;
  cost_used: number;
  remaining: number;
};

export type {
  UsageQuery,
  UsageQueryRow,
};

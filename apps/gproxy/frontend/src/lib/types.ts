export type Provider = {
  id: number;
  name: string;
  config_json: unknown;
  enabled: boolean;
  updated_at: number;
};

export type Credential = {
  id: number;
  provider_id: number;
  name?: string | null;
  secret: unknown;
  meta_json: unknown;
  weight: number;
  enabled: boolean;
  created_at: number;
  updated_at: number;
};

export type DisallowRecord = {
  id: number;
  credential_id: number;
  scope_kind: string;
  scope_value?: string | null;
  level: string;
  until_at?: number | null;
  reason?: string | null;
  updated_at: number;
};

export type User = {
  id: number;
  name?: string | null;
  created_at: number;
  updated_at: number;
};

export type ApiKey = {
  id: number;
  user_id: number;
  key_value: string;
  label?: string | null;
  enabled: boolean;
  created_at: number;
  last_used_at?: number | null;
};

export type ProviderStats = {
  name: string;
  credentials_total: number;
  credentials_enabled: number;
  disallow: number;
};

export type GlobalConfig = {
  host: string;
  port: number;
  admin_key: string;
  dsn: string;
  proxy?: string | null;
  data_dir?: string | null;
};

export type UpstreamUsage = {
  credential_id: number;
  model?: string | null;
  start: number;
  end: number;
  count: number;
  tokens: Record<string, number>;
};

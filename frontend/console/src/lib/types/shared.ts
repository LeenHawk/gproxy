export type ErrorResponse = {
  error: string;
};

export type AckResponse = {
  ok: boolean;
  id?: number;
};

export type CountResponse = {
  count: number;
};

export type Scope<T> = "All" | { Eq: T } | { In: T[] };

export type LoginRequest = {
  username: string;
  password: string;
};

export type LoginResponse = {
  user_id: number;
  session_token: string;
  expires_in_secs: number;
  is_admin: boolean;
};

export type PriceTier = {
  input_tokens_up_to: number;
  price_input_tokens?: number | null;
  price_output_tokens?: number | null;
  price_cache_read_input_tokens?: number | null;
  price_cache_creation_input_tokens?: number | null;
  price_cache_creation_input_tokens_5min?: number | null;
  price_cache_creation_input_tokens_1h?: number | null;
};

export type UsageQuery = {
  provider_id?: Scope<number>;
  credential_id?: Scope<number>;
  channel?: Scope<string>;
  model?: Scope<string>;
  user_id?: Scope<number>;
  user_key_id?: Scope<number>;
  from_unix_ms?: number;
  to_unix_ms?: number;
  cursor_at_unix_ms?: number;
  cursor_trace_id?: number;
  offset?: number;
  limit?: number;
};

export type UsageQueryRow = {
  trace_id: number;
  downstream_trace_id?: number | null;
  at: string;
  provider_id?: number | null;
  provider_channel?: string | null;
  credential_id?: number | null;
  user_id?: number | null;
  user_key_id?: number | null;
  operation: string;
  protocol: string;
  model?: string | null;
  input_tokens?: number | null;
  output_tokens?: number | null;
  cache_read_input_tokens?: number | null;
  cache_creation_input_tokens?: number | null;
  cache_creation_input_tokens_5min?: number | null;
  cache_creation_input_tokens_1h?: number | null;
};

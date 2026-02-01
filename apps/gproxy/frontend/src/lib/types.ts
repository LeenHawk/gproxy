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

export type LogPage<T> = {
  page: number;
  page_size: number;
  has_more: boolean;
  items: T[];
};

export type DownstreamLog = {
  id: number;
  created_at: number;
  provider: string;
  provider_id?: number | null;
  operation: string;
  model?: string | null;
  user_id?: number | null;
  key_id?: number | null;
  trace_id?: string | null;
  request_method: string;
  request_path: string;
  request_query?: string | null;
  request_headers: string;
  request_body: string;
  response_status: number;
  response_headers: string;
  response_body: string;
};

export type UpstreamLog = {
  id: number;
  created_at: number;
  provider: string;
  provider_id?: number | null;
  operation: string;
  model?: string | null;
  credential_id?: number | null;
  trace_id?: string | null;
  request_method: string;
  request_path: string;
  request_query?: string | null;
  request_headers: string;
  request_body: string;
  response_status: number;
  response_headers: string;
  response_body: string;
  claude_input_tokens?: number | null;
  claude_output_tokens?: number | null;
  claude_total_tokens?: number | null;
  claude_cache_creation_input_tokens?: number | null;
  claude_cache_read_input_tokens?: number | null;
  gemini_prompt_tokens?: number | null;
  gemini_candidates_tokens?: number | null;
  gemini_total_tokens?: number | null;
  gemini_cached_tokens?: number | null;
  openai_chat_prompt_tokens?: number | null;
  openai_chat_completion_tokens?: number | null;
  openai_chat_total_tokens?: number | null;
  openai_responses_input_tokens?: number | null;
  openai_responses_output_tokens?: number | null;
  openai_responses_total_tokens?: number | null;
  openai_responses_input_cached_tokens?: number | null;
  openai_responses_output_reasoning_tokens?: number | null;
};

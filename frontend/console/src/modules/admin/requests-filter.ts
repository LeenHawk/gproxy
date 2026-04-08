import { scopeAll } from "../../lib/scope";

export const KNOWN_DOWNSTREAM_REQUEST_PATHS = [
  "/login",
  "/admin/health",
  "/admin/reload",
  "/admin/global-settings",
  "/admin/providers/query",
  "/admin/providers/upsert",
  "/admin/providers/delete",
  "/admin/credentials/query",
  "/admin/credentials/upsert",
  "/admin/credentials/delete",
  "/admin/credential-statuses/query",
  "/admin/credential-statuses/update",
  "/admin/models/query",
  "/admin/models/upsert",
  "/admin/models/delete",
  "/admin/model-aliases/query",
  "/admin/model-aliases/upsert",
  "/admin/model-aliases/delete",
  "/admin/users/query",
  "/admin/users/upsert",
  "/admin/users/delete",
  "/admin/user-keys/query",
  "/admin/user-keys/generate",
  "/admin/user-keys/update-enabled",
  "/admin/user-keys/delete",
  "/admin/user-quotas/query",
  "/admin/user-quotas/upsert",
  "/admin/user-permissions/query",
  "/admin/user-permissions/upsert",
  "/admin/user-permissions/delete",
  "/admin/user-file-permissions/query",
  "/admin/user-file-permissions/upsert",
  "/admin/user-file-permissions/delete",
  "/admin/user-rate-limits/query",
  "/admin/user-rate-limits/upsert",
  "/admin/user-rate-limits/delete",
  "/admin/usages/query",
  "/user/keys/query",
  "/user/keys/generate",
  "/user/keys/update-enabled",
  "/user/keys/delete",
  "/user/quota",
  "/user/usages/query",
  "/user/usages/count",
  "/v1/messages",
  "/v1/chat/completions",
  "/v1/responses",
  "/v1/responses/input_tokens",
  "/v1/responses/compact",
  "/v1/embeddings",
  "/v1/images/generations",
  "/v1/images/edits",
  "/v1/models",
  "/v1/files",
  "/v1/files/{file_id}",
  "/v1/files/{file_id}/content",
  "/v1beta/models",
];

export const KNOWN_UPSTREAM_REQUEST_TARGETS = [
  "/v1/messages",
  "/v1/messages/count_tokens",
  "/v1/chat/completions",
  "/v1/responses",
  "/v1/responses/input_tokens",
  "/v1/responses/compact",
  "/v1/embeddings",
  "/v1/images/generations",
  "/v1/images/edits",
  "/v1/models",
  "/v1/files",
  "/v1/files/",
  "/v1/files/content",
  "/v1beta/models",
  "/v1internal:retrieveUserQuota",
  "/v1internal:loadCodeAssist",
  "/v1internal:onboardUser",
  "/v1/oauth/token",
  "/api/oauth/profile",
  "/api/oauth/usage",
  "/wham/usage",
];

export function buildDownstreamRequestQuery(form: {
  user_id: string;
  user_key_id: string;
  request_path_contains: string;
  limit: string;
  offset?: number;
  include_body: boolean;
}) {
  return {
    trace_id: scopeAll<number>(),
    user_id: form.user_id ? { Eq: Number(form.user_id) } : scopeAll<number>(),
    user_key_id: form.user_key_id ? { Eq: Number(form.user_key_id) } : scopeAll<number>(),
    ...(form.request_path_contains.trim()
      ? { request_path_contains: form.request_path_contains.trim() }
      : {}),
    ...(form.offset && form.offset > 0 ? { offset: form.offset } : {}),
    ...(form.limit.trim() ? { limit: Number(form.limit) } : {}),
    include_body: form.include_body,
  };
}

export function buildDownstreamDeleteAllQuery(form: {
  user_id: string;
  user_key_id: string;
  request_path_contains: string;
  limit: string;
  offset?: number;
  include_body: boolean;
}) {
  return buildDownstreamRequestQuery({
    ...form,
    limit: "",
    offset: 0,
    include_body: false,
  });
}

export function buildUpstreamRequestQuery(form: {
  provider_id: string;
  credential_id: string;
  request_url_contains: string;
  limit: string;
  offset?: number;
  include_body: boolean;
}) {
  return {
    trace_id: scopeAll<number>(),
    provider_id: form.provider_id ? { Eq: Number(form.provider_id) } : scopeAll<number>(),
    credential_id: form.credential_id ? { Eq: Number(form.credential_id) } : scopeAll<number>(),
    ...(form.request_url_contains.trim()
      ? { request_url_contains: form.request_url_contains.trim() }
      : {}),
    ...(form.offset && form.offset > 0 ? { offset: form.offset } : {}),
    ...(form.limit.trim() ? { limit: Number(form.limit) } : {}),
    include_body: form.include_body,
  };
}

export function buildUpstreamDeleteAllQuery(form: {
  provider_id: string;
  credential_id: string;
  request_url_contains: string;
  limit: string;
  offset?: number;
  include_body: boolean;
}) {
  return buildUpstreamRequestQuery({
    ...form,
    limit: "",
    offset: 0,
    include_body: false,
  });
}

export function buildAdminUsageQuery(form: {
  provider_id: string;
  credential_id: string;
  channel: string;
  model: string;
  user_id: string;
  user_key_id: string;
  limit: string;
  offset?: number;
}) {
  return {
    provider_id: form.provider_id ? { Eq: Number(form.provider_id) } : scopeAll<number>(),
    credential_id: form.credential_id ? { Eq: Number(form.credential_id) } : scopeAll<number>(),
    channel: form.channel ? { Eq: form.channel } : scopeAll<string>(),
    model: form.model ? { Eq: form.model } : scopeAll<string>(),
    user_id: form.user_id ? { Eq: Number(form.user_id) } : scopeAll<number>(),
    user_key_id: form.user_key_id ? { Eq: Number(form.user_key_id) } : scopeAll<number>(),
    ...(form.offset && form.offset > 0 ? { offset: form.offset } : {}),
    ...(form.limit.trim() ? { limit: Number(form.limit) } : {}),
  };
}

export function buildAdminUsageDeleteAllQuery(form: {
  provider_id: string;
  credential_id: string;
  channel: string;
  model: string;
  user_id: string;
  user_key_id: string;
  limit: string;
  offset?: number;
}) {
  return buildAdminUsageQuery({
    ...form,
    limit: "",
    offset: 0,
  });
}

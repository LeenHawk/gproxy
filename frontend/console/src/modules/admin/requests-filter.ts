export function buildDownstreamRequestQuery(form: {
  request_path_contains: string;
  limit: string;
  include_body: boolean;
}) {
  return {
    ...(form.request_path_contains.trim()
      ? { request_path_contains: form.request_path_contains.trim() }
      : {}),
    ...(form.limit.trim() ? { limit: Number(form.limit) } : {}),
    include_body: form.include_body,
  };
}

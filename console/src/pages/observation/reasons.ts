export const RESPONSE_REASONS = [
  "refusal", "content_filter",
  "safety", "blocklist", "prohibited_content", "sensitive_personal_information",
  "recitation",
  "cyber", "bio", "frontier_llm", "reasoning_extraction", "general_harms", "authentication_failed", "permission_denied",
  "rate_limited", "quota_exhausted", "invalid_request", "not_found",
  "upstream_error", "timeout", "connection_error", "cancelled",
] as const

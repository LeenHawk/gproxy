export function buildOAuthCallbackQuery(callbackUrl: string): string {
  const trimmed = callbackUrl.trim();
  if (!trimmed) {
    throw new Error("OAuth callback URL is required");
  }
  const params = new URLSearchParams();
  params.set("callback_url", trimmed);
  return `?${params.toString()}`;
}

// A pasted callback is checked before it is sent upstream: the common mistakes are
// pasting the authorization URL itself, or a callback left over from an earlier attempt.
// Comparing `state` against the authorization URL catches the stale one.
export function validateCallbackUrl(pasted: string, authorizeUrl: string): boolean {
  let url: URL
  let authorize: URL
  try {
    url = new URL(pasted.trim())
    authorize = new URL(authorizeUrl)
  } catch {
    return false
  }
  const state = url.searchParams.get("state")
  if (!url.searchParams.get("code") || !state) return false
  if (state !== authorize.searchParams.get("state")) return false
  return url.origin !== authorize.origin || url.pathname !== authorize.pathname
}

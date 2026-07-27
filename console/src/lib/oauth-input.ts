/** Client-side guard for the authcode wizard: require the callback from the
 * current authorization session, not an authorize URL or a stale callback. */
export function validateCallbackUrl(pasted: string, authorizeUrl: string): boolean {
  let url: URL;
  try {
    url = new URL(pasted.trim());
  } catch {
    return false;
  }
  const callbackState = url.searchParams.get("state");
  if (!url.searchParams.get("code") || !callbackState) return false;
  try {
    const auth = new URL(authorizeUrl);
    const expectedState = auth.searchParams.get("state");
    if (!expectedState || callbackState !== expectedState) return false;
    if (url.origin === auth.origin && url.pathname === auth.pathname) return false;
  } catch {
    return false;
  }
  return true;
}

/** Forgiving cookie input (ported v1 UX): accept a full Cookie header dump or the bare
 *  sessionKey value; return `sessionKey=…` or null when absent. */
export function extractSessionKey(pasted: string): string | null {
  const text = pasted.trim();
  if (text === "") return null;
  const match = /sessionKey=([^;\s]+)/.exec(text);
  if (match) return `sessionKey=${match[1]}`;
  if (text.startsWith("sk-ant-") && !text.includes("=") && !text.includes(";")) {
    return `sessionKey=${text}`;
  }
  return null;
}

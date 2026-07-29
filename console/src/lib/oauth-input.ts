import type { ChannelMeta } from "@/lib/channel-meta";

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

function cookieHeaderValue(pasted: string): string {
  const text = pasted.trim();
  const separator = text.indexOf(":");
  return separator > 0 && text.slice(0, separator).trim().toLowerCase() === "cookie"
    ? text.slice(separator + 1).trim()
    : text;
}

/** External cookie values are opaque. Claude's built-ins additionally accept a bare
 * session key while preserving every cookie from a complete browser header. */
export function normalizeCookieInput(
  pasted: string,
  meta: Pick<ChannelMeta, "id" | "source">,
): string | null {
  const isClaudeBuiltin = meta.source === "builtin"
    && (meta.id === "claudecode" || meta.id === "claudeweb");
  if (!isClaudeBuiltin) return pasted.trim() || null;
  const text = cookieHeaderValue(pasted);
  if (text === "") return null;
  if (/((^|;)\s*sessionKey=[^;\s]+)/.test(text)) return text;
  if (text.startsWith("sk-ant-") && !text.includes("=") && !text.includes(";")) {
    return `sessionKey=${text}`;
  }
  return null;
}

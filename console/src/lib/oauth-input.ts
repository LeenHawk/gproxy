/** Client-side guard for the authcode wizard (spec §6): the pasted URL must be the
 *  CALLBACK (carrying code+state) — not the authorize URL itself (known backend wontfix). */
export function validateCallbackUrl(pasted: string, authorizeUrl: string): boolean {
  let url: URL;
  try {
    url = new URL(pasted.trim());
  } catch {
    return false;
  }
  if (!url.searchParams.get("code") || !url.searchParams.get("state")) return false;
  try {
    const auth = new URL(authorizeUrl);
    if (url.origin === auth.origin && url.pathname === auth.pathname) return false;
  } catch {
    /* unparseable authorize URL — fall through */
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

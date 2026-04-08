export function authHeaders(sessionToken: string, contentType = true): Headers {
  const headers = new Headers();
  headers.set("Authorization", `Bearer ${sessionToken}`);
  if (contentType) {
    headers.set("content-type", "application/json");
  }
  return headers;
}

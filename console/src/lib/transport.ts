//! How a console request reaches its host.
//!
//! In a browser that is `fetch`. In the desktop and Android window there is no
//! server to fetch from: the shell answers `desktop_console_request` by running
//! the request through the server's own router in process (see
//! `gproxy-host-tauri/src/console.rs`) and hands back status and body. Either
//! way the caller gets a `Response`, so [`api`](@/api/client) and everything
//! above it cannot tell which host it is talking to.

import { invoke, isTauri } from "@tauri-apps/api/core"

/** `true` inside the Tauri window, where the host is this process. */
export const inShell = isTauri()

type ShellResponse = { status: number; contentType: string | null; body: string }

export async function send(path: string, init: RequestInit): Promise<Response> {
  if (!inShell) return fetch(path, init)
  const signal = init.signal
  signal?.throwIfAborted()
  const answer = invoke<ShellResponse>("desktop_console_request", { request: {
    method: init.method ?? "GET",
    path,
    headers: [...new Headers(init.headers).entries()],
    body: typeof init.body === "string" ? init.body : null,
  } })
  // The bridge cannot be cancelled, but a caller that gave up must not be
  // handed a stale answer — the scope switch in `api` depends on it.
  const reply = await (signal ? Promise.race([answer, new Promise<never>((_, reject) => {
    signal.addEventListener("abort", () => reject(signal.reason), { once: true })
  })]) : answer)
  // `Response` refuses a body on the statuses that must not have one.
  const empty = reply.status === 204 || reply.status === 205 || reply.status === 304
  return new Response(empty ? null : reply.body, {
    status: reply.status,
    headers: reply.contentType ? { "content-type": reply.contentType } : {},
  })
}

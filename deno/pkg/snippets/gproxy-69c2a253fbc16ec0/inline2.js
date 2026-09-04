
export async function gproxyResponsesWebSocketRoundTrip(
  url, headerEntries, frame, noRedirect, timeoutMs
) {
  const headers = new Headers();
  for (const pair of headerEntries) {
    headers.append(pair[0], pair[1]);
  }
  headers.set("Upgrade", "websocket");

  const controller = new AbortController();
  const decoder = new TextDecoder();
  const messages = [];
  const terminal = new Set(["response.completed", "response.done", "response.failed", "error"]);
  let socket = null;
  let timer = null;
  let timedOut = false;

  const roundTrip = (async () => {
    const response = await fetch(url, {
      method: "GET",
      headers,
      redirect: noRedirect ? "manual" : "follow",
      signal: controller.signal,
    });
    socket = response.webSocket;
    if (!socket) {
      throw new Error(`websocket upgrade failed with status ${response.status}`);
    }
    if (typeof socket.accept === "function") {
      socket.accept();
    }

    return await new Promise((resolve, reject) => {
      let settled = false;
      const finish = () => {
        if (settled) return;
        settled = true;
        resolve(messages);
      };
      const fail = (message) => {
        if (settled) return;
        settled = true;
        reject(new Error(message));
      };

      socket.addEventListener("message", (event) => {
        const text = typeof event.data === "string" ? event.data : decoder.decode(event.data);
        messages.push(text);
        let kind = null;
        try { kind = JSON.parse(text)?.type ?? null; } catch (_) {}
        if (terminal.has(kind)) {
          finish();
        }
      });
      socket.addEventListener("close", () => {
        if (settled) return;
        fail("websocket closed before terminal response");
      });
      socket.addEventListener("error", () => fail("websocket error"));

      if (frame != null) {
        try {
          socket.send(frame);
        } catch (error) {
          fail(error?.message ?? String(error));
        }
      }
    });
  })();

  try {
    if (timeoutMs < 0) {
      return await roundTrip;
    }
    const timeout = new Promise((_, reject) => {
      timer = setTimeout(() => {
        timedOut = true;
        controller.abort();
        try { socket?.close(); } catch (_) {}
        reject(new Error(`websocket exceeded ${timeoutMs}ms total_timeout`));
      }, timeoutMs);
    });
    return await Promise.race([roundTrip, timeout]);
  } catch (error) {
    if (timedOut) {
      throw new Error(`websocket exceeded ${timeoutMs}ms total_timeout`);
    }
    throw error;
  } finally {
    if (timer !== null) clearTimeout(timer);
    controller.abort();
    try { socket?.close(); } catch (_) {}
  }
}

import WebSocket from "ws";
import { Readable } from "node:stream";
import { unavailable } from "./server.mjs";

function close(socket, code, reason) {
  // Abnormal / absent close codes cannot be sent on the wire.
  if ([1005, 1006, 1015].includes(code)) code = 1000;
  try { socket.close(code, reason); } catch { socket.close(); }
}

export async function denoFetch(gateway, request, clientIp) {
  if (request.headers.get("upgrade")?.toLowerCase() !== "websocket") {
    return gateway.fetch(request, clientIp);
  }
  // Return the public 101 only after Rust has authenticated the request
  // and accepted the upstream connection.
  const protocols = request.headers.get("sec-websocket-protocol")?.split(",").map(s => s.trim()) ?? [];
  const target = await gateway.websocketTarget(request, clientIp);
  for (const header of ["sec-websocket-key", "sec-websocket-version", "sec-websocket-protocol", "sec-websocket-extensions"]) {
    delete target.headers[header];
  }
  return new Promise(resolve => {
    const upstream = new WebSocket(target.url, protocols, { headers: target.headers, perMessageDeflate: false });
    let downstream;
    const abort = () => upstream.terminate();
    request.signal.addEventListener("abort", abort, { once: true });
    if (request.signal.aborted) abort();
    upstream.on("error", () => {
      if (downstream) close(downstream, 1011, "Upstream disconnected");
      else resolve(unavailable());
    });
    upstream.once("unexpected-response", (_request, response) => {
      const headers = new Headers();
      for (let i = 0; i < response.rawHeaders.length; i += 2) {
        const name = response.rawHeaders[i];
        if (!["connection", "transfer-encoding", "upgrade"].includes(name.toLowerCase())) {
          headers.append(name, response.rawHeaders[i + 1]);
        }
      }
      response.once("end", () => upstream.terminate());
      resolve(new Response(Readable.toWeb(response), { status: response.statusCode, headers }));
    });
    upstream.once("open", () => {
      let upgrade;
      try {
        // Deno aborts the HTTP request signal when ownership moves to the
        // WebSocket. From here, socket close events own connection cleanup.
        request.signal.removeEventListener("abort", abort);
        upgrade = Deno.upgradeWebSocket(request, {
          protocol: upstream.protocol || undefined,
          idleTimeout: 0,
        });
      } catch {
        upstream.terminate();
        resolve(new Response("Invalid WebSocket handshake", { status: 400 }));
        return;
      }
      downstream = upgrade.socket;
      downstream.binaryType = "arraybuffer";
      const pending = [];
      upstream.on("message", (data, binary) => {
        const message = binary ? data : data.toString();
        if (downstream.readyState === 0) pending.push(message);
        else if (downstream.readyState === 1) downstream.send(message);
      });
      downstream.onopen = () => {
        for (const message of pending) downstream.send(message);
        pending.length = 0;
        if (upstream.readyState !== WebSocket.OPEN) close(downstream, 1000, "");
      };
      downstream.onmessage = event => {
        if (upstream.readyState === WebSocket.OPEN) upstream.send(event.data);
      };
      downstream.onclose = event => close(upstream, event.code, event.reason);
      downstream.onerror = () => upstream.terminate();
      resolve(upgrade.response);
    });
    upstream.once("close", (code, reason) => {
      request.signal.removeEventListener("abort", abort);
      if (downstream?.readyState === 1) close(downstream, code, reason.toString());
    });
  });
}

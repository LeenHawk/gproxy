import { createServer, request as httpRequest } from "node:http";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";
import { unavailable } from "./server.mjs";

function inputRequest(request, signal) {
  const url = `https://${request.headers.host}${request.url}`;
  const body = ["GET", "HEAD"].includes(request.method) ? undefined : Readable.toWeb(request);
  return new Request(url, { method: request.method, headers: request.headers,
    body, duplex: "half", signal });
}

async function send(result, response) {
  response.statusCode = result.status;
  for (const [name, value] of result.headers) {
    if (name !== "set-cookie") response.setHeader(name, value);
  }
  const cookies = result.headers.getSetCookie();
  if (cookies.length) response.setHeader("set-cookie", cookies);
  if (result.body) await pipeline(Readable.fromWeb(result.body), response);
  else response.end();
}

async function rejectUpgrade(socket, status, statusMessage, rawHeaders, body) {
  const headers = [];
  for (let i = 0; i < rawHeaders.length; i += 2) {
    // IncomingMessage has already decoded HTTP chunk framing. Delimit this
    // failed handshake by closing the connection instead of forwarding it.
    if (!["connection", "transfer-encoding"].includes(rawHeaders[i].toLowerCase())) {
      headers.push(`${rawHeaders[i]}: ${rawHeaders[i + 1]}`);
    }
  }
  socket.write(`HTTP/1.1 ${status} ${statusMessage}\r\n${headers.join("\r\n")}\r\nConnection: close\r\n\r\n`);
  try { await pipeline(body, socket); } catch { socket.destroy(); }
}

export function createNodeServer(gateway, clientIp) {
  const server = createServer(async (request, response) => {
    const controller = new AbortController();
    response.once("close", () => {
      if (!response.writableFinished) controller.abort();
    });
    let result;
    try {
      result = await gateway.fetch(inputRequest(request, controller.signal), clientIp(request));
    } catch {
      result = unavailable();
    }
    try { await send(result, response); } catch { response.destroy(); }
  });

  server.on("upgrade", async (request, socket, head) => {
    const controller = new AbortController();
    socket.once("close", () => controller.abort());
    socket.on("error", () => socket.destroy());
    let upgraded = false;
    let responded = false;
    const reject = async () => {
      if (socket.destroyed || responded) return;
      responded = true;
      const result = unavailable();
      await rejectUpgrade(socket, result.status, "Service Unavailable",
        [...result.headers].flat(), Readable.fromWeb(result.body));
    };
    try {
      const target = await gateway.websocketTarget(inputRequest(request, controller.signal), clientIp(request));
      const upstream = httpRequest(target.url, {
        method: request.method,
        headers: { ...target.headers, connection: "Upgrade", upgrade: "websocket" },
        signal: controller.signal,
      });
      upstream.once("error", () => {
        if (upgraded) socket.destroy();
        else void reject();
      });
      upstream.once("response", result => {
        // Preserve authentication and routing failures before sending any 101.
        responded = true;
        void rejectUpgrade(socket, result.statusCode, result.statusMessage, result.rawHeaders, result);
      });
      upstream.once("upgrade", (result, peer, peerHead) => {
        upgraded = responded = true;
        peer.on("error", () => socket.destroy());
        peer.once("close", () => socket.destroy());
        socket.once("close", () => peer.destroy());
        if (socket.destroyed) { peer.destroy(); return; }
        const headers = [];
        for (let i = 0; i < result.rawHeaders.length; i += 2) {
          headers.push(`${result.rawHeaders[i]}: ${result.rawHeaders[i + 1]}`);
        }
        socket.write(`HTTP/1.1 101 ${result.statusMessage}\r\n${headers.join("\r\n")}\r\n\r\n`);
        // Both parsers may already have read the first frame with the handshake.
        if (peerHead.length) socket.write(peerHead);
        if (head.length) peer.write(head);
        socket.pipe(peer).pipe(socket);
      });
      upstream.end();
    } catch {
      await reject();
    }
  });
  return server;
}

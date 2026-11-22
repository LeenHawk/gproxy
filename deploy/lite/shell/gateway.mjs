// The shell around the gproxy-lite WebAssembly module: it starts the engine
// once per instance and hands it the platform's requests.

import { initSync, Lite } from "./pkg/gproxy_lite.js";

function refuse(status, message) {
  return Response.json({ error: { message, type: "invalid_request_error" } }, { status });
}

// `wasm` is a WebAssembly.Module, `config` the configuration JSON, `store`
// the platform's key-value store (see ./stores). `websocket`, where the
// platform can accept one, takes the handshake request and a function that
// serves the server end of the socket.
export function createGateway({ wasm, config, store, websocket }) {
  if (!config) throw new Error("GPROXY_LITE_CONFIG is not set");
  initSync({ module: wasm });
  const lite = new Lite(config, store);
  return {
    fetch(request) {
      if (request.headers.get("upgrade")?.toLowerCase() !== "websocket") {
        return lite.fetch(request);
      }
      if (!websocket) return refuse(501, "this platform cannot serve WebSockets");
      return websocket(request, (handshake, socket) => lite.websocket(handshake, socket));
    },
    // `[{ provider, outcome, reason? }]` for every credential that can be renewed.
    renew: () => lite.renew(),
  };
}

// What the platform's scheduler runs: renew the credentials that are due, so
// a sign-in outlives a period without requests. `started` is what `lazily`
// resolved to. A failure is thrown after the report is logged, so the
// platform records the run as failed.
export async function renewDue(started) {
  if (started instanceof Response) throw new Error(await started.text());
  const renewals = await started.renew();
  console.log(JSON.stringify({ renewals }));
  const failed = renewals.filter(renewal => renewal.outcome === "failed");
  if (failed.length > 0) {
    throw new Error(failed.map(renewal => `${renewal.provider}: ${renewal.reason}`).join("; "));
  }
  return renewals;
}

// One gateway per instance. A configuration the engine refuses is answered on
// every request, so the operator reads why instead of a blank 500.
export function lazily(create) {
  let gateway;
  return async (...context) => {
    try {
      gateway ??= await create(...context);
    } catch (error) {
      return refuse(500, `gproxy-lite did not start: ${error.message ?? error}`);
    }
    return gateway;
  };
}

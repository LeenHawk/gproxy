// GPROXY v2 — Cloudflare Workers (module worker) entry.
//
// Cloudflare Workers use a static-wasm-module model: a
// statically-imported `.wasm` is bundled by wrangler as a `WebAssembly.Module`
// (no `?module` suffix on CF), and runtime byte compilation of arbitrary
// buffers is forbidden. So this entry reuses the wasm-bindgen `--target web`
// glue and hands it the bundled Module:
//
//   import wasmModule from "./_lib/gproxy_bg.wasm";           // WebAssembly.Module
//   import initWasm, { fetch as wasmFetch, init as gproxyInit } from "./_lib/gproxy.js";
//   await initWasm({ module_or_path: wasmModule });           // WebAssembly.instantiate(Module, imports)
//
// The web-target default export (`__wbg_init`) routes a `WebAssembly.Module`
// straight to `WebAssembly.instantiate(module, imports)` (no fetch of the
// .wasm), satisfying the Workers sandbox.
//
// Unlike Netlify (Netlify.env), a module worker receives
// its bindings via the `env` ARGUMENT of `fetch(request, env, ctx)` — secrets
// set with `wrangler secret put` and `[vars]` from wrangler.toml both land
// there. So `ensureReady` reads creds from `env`, NOT a global.
//
// Credentials (set with `echo -n "$VALUE" | wrangler secret put NAME`):
//   TURSO_URL, TURSO_TOKEN          (required — libSQL/Turso persistence)
//   GPROXY_ADMIN_USER, GPROXY_ADMIN_PASSWORD
//                                  (required — first admin login)
//   UPSTASH_URL, UPSTASH_TOKEN      (optional — Upstash Redis cache)
//   GPROXY_MASTER_KEY               (optional — unseals encrypted stored
//                                    secrets; absent → plaintext mode)
//
// The generated glue (_lib/gproxy.js + gproxy_bg.wasm + *.d.ts) is gitignored;
// only this file + wrangler.toml + build.sh are hand-written source. Build
// from the crate root, then run wrangler from deploy/cloudflare/:
//   cargo rustc --lib --crate-type cdylib --target wasm32-unknown-unknown --release --no-default-features --features edge
//   bash deploy/cloudflare/build.sh

import wasmModule from "./_lib/gproxy_bg.wasm";
import initWasm, {
  fetch as wasmFetch,
  init as gproxyInit,
  responses_websocket_frame as wasmResponsesWebSocketFrame,
} from "./_lib/gproxy.js";

function reqEnv(env, name) {
  const v = env[name];
  if (!v) {
    throw new Error(`missing required env var: ${name}`);
  }
  return v;
}

function optEnv(env, name) {
  const v = env[name];
  return v && v.length > 0 ? v : undefined;
}

// Instantiate the wasm Module + build the shared AppState exactly once, LAZILY
// on the first request — the worker bindings (`env`) are only populated at
// request time, and the Rust `init` is itself idempotent (first AppState wins).
let ready;

function ensureReady(env) {
  if (!ready) {
    ready = (async () => {
      // Pass the bundled WebAssembly.Module — the web-target loader sends it to
      // WebAssembly.instantiate(module, imports) (no byte compile, no URL fetch).
      await initWasm({ module_or_path: wasmModule });
      await gproxyInit(
        reqEnv(env, "TURSO_URL"),
        reqEnv(env, "TURSO_TOKEN"),
        optEnv(env, "UPSTASH_URL"),
        optEnv(env, "UPSTASH_TOKEN"),
        optEnv(env, "GPROXY_MASTER_KEY"),
        reqEnv(env, "GPROXY_ADMIN_USER"),
        reqEnv(env, "GPROXY_ADMIN_PASSWORD"),
      );
    })();
  }
  return ready;
}

const ROOT_ASSET_PATHS = new Set([
  "/favicon.ico",
  "/favicon-96x96.png",
  "/apple-touch-icon.png",
]);

function isConsolePath(pathname) {
  return (
    pathname === "/" ||
    pathname === "/console" ||
    pathname === "/console/" ||
    pathname.startsWith("/console/") ||
    ROOT_ASSET_PATHS.has(pathname)
  );
}

function hasFileExtension(pathname) {
  const last = pathname.split("/").pop() ?? "";
  return last.includes(".");
}

function redirectToConsole(request) {
  const url = new URL(request.url);
  url.pathname = "/console/";
  url.search = "";
  return Response.redirect(url.toString(), 308);
}

function isResponsesWebSocketPath(pathname) {
  if (pathname === "/v1/responses") {
    return true;
  }
  const parts = pathname.split("/").filter(Boolean);
  return (
    parts.length === 3 &&
    parts[1] === "v1" &&
    parts[2] === "responses" &&
    !["v1", "v1beta", "console"].includes(parts[0])
  );
}

function isWebSocketRequest(request) {
  return request.headers.get("upgrade")?.toLowerCase() === "websocket";
}

function websocketErrorFrame(status, message) {
  return JSON.stringify({
    type: "error",
    status,
    status_code: status,
    error: { message, type: "gproxy_error" },
  });
}

async function serveResponsesWebSocket(request) {
  const pair = new WebSocketPair();
  const [client, server] = Object.values(pair);
  const decoder = new TextDecoder();
  let chain = Promise.resolve();

  server.accept();
  server.addEventListener("message", (event) => {
    chain = chain
      .then(async () => {
        const frame =
          typeof event.data === "string" ? event.data : decoder.decode(event.data);
        const messages = await wasmResponsesWebSocketFrame(request, frame);
        for (const message of messages) {
          server.send(message);
        }
      })
      .catch((err) => {
        console.error("responses websocket frame failed", err);
        try {
          server.send(websocketErrorFrame(500, "websocket frame failed"));
        } catch (_) {
          try {
            server.close(1011, "websocket frame failed");
          } catch (_) {}
        }
      });
  });
  server.addEventListener("error", (event) => {
    console.error("responses websocket error", event.error ?? event);
  });

  return new Response(null, { status: 101, webSocket: client });
}

async function serveConsole(request, env) {
  if (!env.ASSETS) {
    return new Response("console assets not bundled", {
      status: 404,
      headers: { "content-type": "text/plain; charset=utf-8" },
    });
  }

  const url = new URL(request.url);
  if (url.pathname === "/" || url.pathname === "/console") {
    return redirectToConsole(request);
  }

  let assetRequest = request;
  let indexFallback = false;

  // Match native's SPA fallback: real dotted files 404 when absent; extensionless
  // /console routes serve index.html so TanStack Router can resolve them.
  if (url.pathname === "/console/" || !hasFileExtension(url.pathname)) {
    // Cloudflare Assets canonicalizes *.html requests to extensionless URLs.
    // Use an extensionless internal copy for SPA fallbacks; it still loads
    // assets from /console/assets because Vite builds with base: /console/.
    url.pathname = "/__gproxy_console";
    assetRequest = new Request(url.toString(), request);
    indexFallback = true;
  }

  const response = await env.ASSETS.fetch(assetRequest);
  if (!indexFallback) {
    return response;
  }

  const headers = new Headers(response.headers);
  headers.set("content-type", "text/html; charset=utf-8");
  headers.set("cache-control", "no-cache");
  return new Response(response.body, {
    status: response.status,
    statusText: response.statusText,
    headers,
  });
}

export default {
  async fetch(request, env, _ctx) {
    const path = new URL(request.url).pathname;
    if (isConsolePath(path)) {
      return serveConsole(request, env);
    }

    await ensureReady(env);
    if (isWebSocketRequest(request) && isResponsesWebSocketPath(path)) {
      return serveResponsesWebSocket(request);
    }
    // The wasm router matches bare paths (`/healthz`, `/version`); the worker
    // receives the original request URL unchanged, so paths pass straight through.
    return wasmFetch(request);
  },
};

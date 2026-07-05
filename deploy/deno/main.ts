// GPROXY v2 — Deno Deploy production entry.
//
// Loads the wasm-bindgen `deno`-target glue from ../../pkg (regenerated, never
// committed — see the build recipe below), wires the storage credentials from
// the Deno Deploy environment into the Rust `init(...)`, then serves every
// inbound request through the wasm `fetch` export (the SAME http::server::router
// native uses).
//
// Credentials are read from Deno Deploy env vars at module load — NEVER hard-
// coded here:
//   TURSO_URL, TURSO_TOKEN          (required — libSQL/Turso persistence)
//   GPROXY_ADMIN_USER, GPROXY_ADMIN_PASSWORD
//                                  (required — first admin login)
//   UPSTASH_URL, UPSTASH_TOKEN      (optional — Upstash Redis cache; falls
//                                    back to the libSQL kv table when absent)
//   GPROXY_MASTER_KEY               (optional — unseals encrypted stored
//                                    secrets; absent → plaintext mode)
//
// Build recipe (run from the crate root before deploying; pkg/ is gitignored):
//   cargo build --lib --target wasm32-unknown-unknown --release --no-default-features --features edge
//   wasm-bindgen --target deno --out-dir pkg \
//     target/wasm32-unknown-unknown/release/gproxy.wasm
//   # The crate exports a fn named `fetch` (the WinterCG entry point), which
//   # shadows the global `fetch` that wasm-bindgen's deno loader uses to read
//   # the .wasm at import ("Cannot access 'wasm' before initialization"). Force
//   # the loader to use the global explicitly:
//   perl -0pi -e \
//     's/instantiateStreaming\(fetch\(wasmUrl\)/instantiateStreaming(globalThis.fetch(wasmUrl)/' \
//     pkg/gproxy.js
//
// Deploy with deploy/deno/build.sh. The script builds a temporary upload root
// whose main.ts imports ./pkg/gproxy.js, matching Deno Deploy's app build
// configuration, and copies the Console build to ./console so this entry can
// serve the same-origin web UI.
//
// `wasmFetch` is aliased from the wasm `fetch` export so it does not shadow
// Deno's global `fetch`, which the glue's loader still needs at import time.

import { fetch as wasmFetch, init } from "../../pkg/gproxy.js";

function reqEnv(name: string): string {
  const v = Deno.env.get(name);
  if (!v) {
    throw new Error(`missing required env var: ${name}`);
  }
  return v;
}

function optEnv(name: string): string | undefined {
  const v = Deno.env.get(name);
  return v && v.length > 0 ? v : undefined;
}

// Build the shared AppState lazily so Console static assets can still be served
// before the first API/gateway request initialises the wasm router.
let initialised: Promise<void> | undefined;

function ensureInit(): Promise<void> {
  if (!initialised) {
    initialised = init(
      reqEnv("TURSO_URL"),
      reqEnv("TURSO_TOKEN"),
      optEnv("UPSTASH_URL"),
      optEnv("UPSTASH_TOKEN"),
      optEnv("GPROXY_MASTER_KEY"),
      reqEnv("GPROXY_ADMIN_USER"),
      reqEnv("GPROXY_ADMIN_PASSWORD"),
    );
  }
  return initialised;
}

const CONSOLE_DIR = new URL("./console/", import.meta.url);
const ROOT_ASSET_PATHS = new Set([
  "/favicon.ico",
  "/favicon-96x96.png",
  "/apple-touch-icon.png",
]);

function isConsolePath(pathname: string): boolean {
  return (
    pathname === "/" ||
    pathname === "/console" ||
    pathname === "/console/" ||
    pathname.startsWith("/console/") ||
    ROOT_ASSET_PATHS.has(pathname)
  );
}

function hasFileExtension(pathname: string): boolean {
  const last = pathname.split("/").pop() ?? "";
  return last.includes(".");
}

function contentType(pathname: string): string {
  if (pathname.endsWith(".html")) return "text/html; charset=utf-8";
  if (pathname.endsWith(".css")) return "text/css; charset=utf-8";
  if (pathname.endsWith(".js")) return "application/javascript; charset=utf-8";
  if (pathname.endsWith(".json")) return "application/json; charset=utf-8";
  if (pathname.endsWith(".svg")) return "image/svg+xml";
  if (pathname.endsWith(".png")) return "image/png";
  if (pathname.endsWith(".ico")) return "image/x-icon";
  if (pathname.endsWith(".woff2")) return "font/woff2";
  return "application/octet-stream";
}

function redirectToConsole(req: Request): Response {
  const url = new URL(req.url);
  url.pathname = "/console/";
  url.search = "";
  return Response.redirect(url.toString(), 308);
}

async function serveConsole(req: Request): Promise<Response> {
  const url = new URL(req.url);
  if (url.pathname === "/" || url.pathname === "/console") {
    return redirectToConsole(req);
  }

  let rel = ROOT_ASSET_PATHS.has(url.pathname)
    ? url.pathname.slice(1)
    : url.pathname.replace(/^\/console\/?/, "");
  let indexFallback = false;

  if (!rel || !hasFileExtension(url.pathname)) {
    rel = "index.html";
    indexFallback = true;
  }

  if (rel.split("/").some((part) => part === "..")) {
    return new Response("not found", { status: 404 });
  }

  try {
    const body = await Deno.readFile(new URL(rel, CONSOLE_DIR));
    const headers = new Headers({ "content-type": contentType(rel) });
    if (indexFallback || rel === "index.html") {
      headers.set("cache-control", "no-cache");
    } else if (rel.startsWith("assets/")) {
      headers.set("cache-control", "public, max-age=31536000, immutable");
    } else {
      headers.set("cache-control", "public, max-age=3600");
    }
    return new Response(body, { headers });
  } catch {
    if (!indexFallback && hasFileExtension(url.pathname)) {
      return new Response("not found", { status: 404 });
    }
    try {
      const body = await Deno.readFile(new URL("index.html", CONSOLE_DIR));
      return new Response(body, {
        headers: {
          "content-type": "text/html; charset=utf-8",
          "cache-control": "no-cache",
        },
      });
    } catch {
      return new Response("console assets not bundled", { status: 404 });
    }
  }
}

Deno.serve(async (req: Request) => {
  const path = new URL(req.url).pathname;
  if (isConsolePath(path)) {
    return serveConsole(req);
  }
  await ensureInit();
  return wasmFetch(req);
});

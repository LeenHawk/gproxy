import init, { EdgeConfig, start } from "./pkg/gproxy_host_edge.js"

const publicRoot = new URL("./public/", import.meta.url)
const wasmReady = Deno.readFile(new URL("./pkg/gproxy_host_edge_bg.wasm", import.meta.url))
  .then((bytes) => init(bytes))
const continuations = new Set<Promise<unknown>>()
let hostPromise: ReturnType<typeof start> | undefined

async function host() {
  await wasmReady
  const config = new EdgeConfig(
    required("GPROXY_LIBSQL_URL"),
    required("GPROXY_LIBSQL_AUTH_TOKEN"),
    Deno.env.get("GPROXY_SECRET_KEY"),
    Deno.env.get("GPROXY_SECRET_KEY_NEXT"),
    rotationArmed(Deno.env.get("GPROXY_SECRET_KEY_ROTATE")),
  )
  hostPromise ??= start(config)
  return hostPromise
}

function required(name: string) {
  const value = Deno.env.get(name)
  if (!value) throw new Error(`${name} is not configured`)
  return value
}

function rotationArmed(value?: string) {
  return ["1", "true", "yes", "on"].includes(value?.trim().toLowerCase() ?? "")
}

function staticFile(request: Request) {
  if (request.method !== "GET" && request.method !== "HEAD") return null
  const path = new URL(request.url).pathname
  if (path === "/" || path === "/admin" || path === "/admin/" || path === "/portal" || path === "/portal/") {
    return "index.html"
  }
  if (path === "/favicon.svg") return "favicon.svg"
  if (/^\/assets\/[A-Za-z0-9._-]+$/.test(path)) return path.slice(1)
  return null
}

async function serveStatic(request: Request, relative: string) {
  try {
    const bytes = await Deno.readFile(new URL(relative, publicRoot))
    const headers = new Headers({
      "content-type": contentType(relative),
      "cache-control": relative === "index.html"
        ? "no-cache"
        : "public, max-age=31536000, immutable",
    })
    return new Response(request.method === "HEAD" ? null : bytes, { headers })
  } catch (error) {
    if (error instanceof Deno.errors.NotFound) return new Response("not found", { status: 404 })
    throw error
  }
}

function contentType(path: string) {
  const extension = path.slice(path.lastIndexOf("."))
  return {
    ".css": "text/css; charset=utf-8",
    ".html": "text/html; charset=utf-8",
    ".js": "text/javascript; charset=utf-8",
    ".json": "application/json; charset=utf-8",
    ".svg": "image/svg+xml",
    ".wasm": "application/wasm",
    ".woff2": "font/woff2",
  }[extension] ?? "application/octet-stream"
}

function retain(continuation: Promise<unknown>) {
  continuations.add(continuation)
  void continuation.finally(() => continuations.delete(continuation)).catch(() => undefined)
}

Deno.serve(async (request, info) => {
  const relative = staticFile(request)
  if (relative) return serveStatic(request, relative)
  try {
    const runtime = await host()
    const trustedSource = "hostname" in info.remoteAddr ? info.remoteAddr.hostname : "local"
    const reply = await runtime.fetch(request, trustedSource)
    if (reply.continuation) retain(reply.continuation)
    return reply.takeResponse() ?? new Response("edge host returned no response", { status: 500 })
  } catch {
    return Response.json({ error: { message: "edge request failed" } }, { status: 500 })
  }
})

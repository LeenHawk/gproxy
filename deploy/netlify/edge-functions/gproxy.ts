import type { Context } from "@netlify/edge-functions"
import init, { EdgeConfig, start } from "../pkg/gproxy_host_edge.js"

declare const Deno: {
  readFile(path: URL): Promise<Uint8Array>
}

const wasmReady = Deno.readFile(new URL("../pkg/gproxy_host_edge_bg.wasm", import.meta.url))
  .then((bytes) => init(bytes))
let hostPromise: ReturnType<typeof start> | undefined

async function host() {
  await wasmReady
  const config = new EdgeConfig(
    required("GPROXY_LIBSQL_URL"),
    required("GPROXY_LIBSQL_AUTH_TOKEN"),
    Netlify.env.get("GPROXY_MASTER_KEY"),
    Netlify.env.get("GPROXY_MASTER_KEY_NEXT"),
    rotationArmed(Netlify.env.get("GPROXY_MASTER_KEY_ROTATE")),
    Netlify.env.get("UPSTASH_URL"),
    Netlify.env.get("UPSTASH_TOKEN"),
  )
  hostPromise ??= start(config)
  return hostPromise
}

function required(name: string) {
  const value = Netlify.env.get(name)
  if (!value) throw new Error(`${name} is not configured`)
  return value
}

function rotationArmed(value?: string) {
  return ["1", "true", "yes", "on"].includes(value?.trim().toLowerCase() ?? "")
}

function isStatic(request: Request) {
  if (request.method !== "GET" && request.method !== "HEAD") return false
  const path = new URL(request.url).pathname
  return path === "/"
    || path === "/admin"
    || (path.startsWith("/admin/") && path !== "/admin/api" && !path.startsWith("/admin/api/"))
    || path === "/portal"
    || path === "/portal/"
    || path === "/favicon.svg"
    || path.startsWith("/assets/")
}

export default async (request: Request, context: Context) => {
  if (isStatic(request)) {
    const path = new URL(request.url).pathname
    return path === "/admin" || path.startsWith("/admin/")
      ? context.rewrite(new URL("/", request.url))
      : context.next()
  }
  try {
    const runtime = await host()
    const reply = await runtime.fetch(request, context.ip)
    if (reply.continuation) context.waitUntil(reply.continuation)
    return reply.takeResponse() ?? new Response("edge host returned no response", { status: 500 })
  } catch {
    return Response.json({ error: { message: "edge request failed" } }, { status: 500 })
  }
}

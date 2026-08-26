import type { Context } from "@netlify/edge-functions"
import init, { start } from "../pkg/gproxy_host_edge.js"

declare const Deno: {
  readFile(path: URL): Promise<Uint8Array>
}

const wasmReady = Deno.readFile(new URL("../pkg/gproxy_host_edge_bg.wasm", import.meta.url))
  .then((bytes) => init(bytes))
let hostPromise: ReturnType<typeof start> | undefined

async function host() {
  await wasmReady
  const config = Netlify.env.get("GPROXY_CONFIG")
  if (!config) throw new Error("GPROXY_CONFIG is not configured")
  hostPromise ??= start(config)
  return hostPromise
}

function isStatic(request: Request) {
  if (request.method !== "GET" && request.method !== "HEAD") return false
  const path = new URL(request.url).pathname
  return path === "/"
    || path === "/admin"
    || path === "/admin/"
    || path === "/portal"
    || path === "/portal/"
    || path === "/favicon.svg"
    || path.startsWith("/assets/")
}

export default async (request: Request, context: Context) => {
  if (isStatic(request)) return context.next()
  try {
    const runtime = await host()
    const reply = await runtime.fetch(request, context.ip)
    if (reply.continuation) context.waitUntil(reply.continuation)
    return reply.takeResponse() ?? new Response("edge host returned no response", { status: 500 })
  } catch {
    return Response.json({ error: { message: "edge request failed" } }, { status: 500 })
  }
}

import { start } from "../pkg/gproxy_host_edge.js"

interface Env {
  ASSETS: Fetcher
  GPROXY_CONFIG: string
}

let hostPromise: ReturnType<typeof start> | undefined

function host(env: Env) {
  hostPromise ??= start(env.GPROXY_CONFIG)
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

export default {
  async fetch(request: Request, env: Env, context: ExecutionContext) {
    if (isStatic(request)) return env.ASSETS.fetch(request)

    const runtime = await host(env)
    const trustedSource = request.headers.get("cf-connecting-ip") ?? "unknown"
    const reply = await runtime.fetch(request, trustedSource)
    const continuation = reply.continuation
    if (continuation) context.waitUntil(continuation)
    return reply.takeResponse() ?? new Response("edge host returned no response", { status: 500 })
  },
} satisfies ExportedHandler<Env>

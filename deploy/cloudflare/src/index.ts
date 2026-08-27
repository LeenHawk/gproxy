import { EdgeConfig, start } from "../pkg/gproxy_host_edge.js"

interface Env {
  ASSETS: Fetcher
  GPROXY_LIBSQL_URL: string
  GPROXY_LIBSQL_AUTH_TOKEN: string
  GPROXY_SECRET_KEY?: string
  GPROXY_SECRET_KEY_NEXT?: string
  GPROXY_SECRET_KEY_ROTATE?: string
}

let hostPromise: ReturnType<typeof start> | undefined

function host(env: Env) {
  const config = new EdgeConfig(
    env.GPROXY_LIBSQL_URL,
    env.GPROXY_LIBSQL_AUTH_TOKEN,
    env.GPROXY_SECRET_KEY,
    env.GPROXY_SECRET_KEY_NEXT,
    rotationArmed(env.GPROXY_SECRET_KEY_ROTATE),
  )
  hostPromise ??= start(config)
  return hostPromise
}

function rotationArmed(value?: string) {
  return ["1", "true", "yes", "on"].includes(value?.trim().toLowerCase() ?? "")
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

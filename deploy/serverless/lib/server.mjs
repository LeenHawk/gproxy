import { spawn } from "node:child_process";
import { request as httpRequest } from "node:http";
import { isIP } from "node:net";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { Readable } from "node:stream";

const hopHeaders = ["connection", "keep-alive", "proxy-authenticate", "proxy-authorization",
  "te", "trailer", "transfer-encoding", "upgrade"];

export function forwardHeaders(request, clientIp) {
  const headers = new Headers(request.headers);
  const connection = headers.get("connection")?.split(",").map(value => value.trim()) ?? [];
  for (const name of [...hopHeaders, ...connection, "forwarded", "x-forwarded-for",
    "x-forwarded-host", "x-forwarded-proto", "x-real-ip", "cf-connecting-ip"]) {
    headers.delete(name);
  }
  const url = new URL(request.url);
  headers.set("host", url.host);
  headers.set("x-forwarded-proto", url.protocol.slice(0, -1));
  headers.set("x-forwarded-for", isIP(clientIp ?? "") ? clientIp : "0.0.0.0");
  return Object.fromEntries(headers);
}

export function databaseUrl(env) {
  const url = env.GPROXY_DATABASE_URL || env.NETLIFY_DB_URL || env.DATABASE_URL || env.POSTGRES_URL;
  if (!url || !/^postgres(?:ql)?:\/\//.test(url)) {
    throw new Error("Set GPROXY_DATABASE_URL to a PostgreSQL connection string, or attach the platform database.");
  }
  return url;
}

export function createGateway({
  env = process.env,
  binary = join(process.cwd(), ".gproxy", process.arch, "gproxy-serverless"),
  startTimeout = 45_000,
} = {}) {
  let child;
  let ready;
  async function start(origin) {
    if (ready) return ready;
    const dsn = databaseUrl(env);
    if (!env.GPROXY_ADMIN_PASSWORD || !env.GPROXY_MASTER_KEY) {
      throw new Error("Set GPROXY_ADMIN_PASSWORD and GPROXY_MASTER_KEY before deploying.");
    }
    ready = new Promise((resolve, reject) => {
      child = spawn(binary, [], {
        env: {
          ...env,
          GPROXY_DSN: dsn,
          GPROXY_PERSISTENCE: "postgres",
          GPROXY_PUBLIC_BASE_URL: env.GPROXY_PUBLIC_BASE_URL || origin,
        },
        stdio: ["ignore", "pipe", "inherit"],
      });
      const processHandle = child;
      const lines = createInterface({ input: child.stdout });
      const timer = setTimeout(() => {
        processHandle.kill();
        reject(new Error("GPROXY startup timed out; check the database connection and function logs."));
      }, startTimeout);
      const cleanup = () => { clearTimeout(timer); lines.close(); };
      lines.on("line", line => {
        if (/^GPROXY_READY http:\/\/127\.0\.0\.1:\d+$/.test(line)) {
          cleanup();
          // A warm child must not keep a completed function invocation alive.
          // Active HTTP requests still own their sockets until streaming ends.
          processHandle.unref();
          processHandle.stdout.unref?.();
          resolve(line.slice("GPROXY_READY ".length));
        }
      });
      child.once("error", () => {
        cleanup();
        reject(new Error("GPROXY executable could not start; rebuild this deployment."));
      });
      child.once("exit", () => {
        cleanup();
        if (child === processHandle) { child = undefined; ready = undefined; }
        reject(new Error("GPROXY exited during startup; check function logs."));
      });
    }).catch(error => { ready = undefined; throw error; });
    return ready;
  }

  async function fetch(request, clientIp) {
    if (request.headers.get("upgrade")?.toLowerCase() === "websocket") {
      return Response.json({ error: { message: "This deployment does not support WebSocket upgrades. Use HTTP streaming." } }, { status: 501 });
    }
    const url = new URL(request.url);
    if (url.pathname === "/") return Response.redirect(new URL("/console/", url), 302);
    const origin = await start(url.origin);
    // node:http preserves compressed bytes and repeated Set-Cookie headers.
    // fetch would transparently decompress without fixing Content-Encoding.
    return new Promise((resolve, reject) => {
      const upstream = httpRequest(`${origin}${url.pathname}${url.search}`, {
        method: request.method,
        headers: forwardHeaders(request, clientIp),
        signal: request.signal,
      }, response => {
        const headers = new Headers();
        const excluded = new Set([...hopHeaders,
          ...(response.headers.connection ?? "").split(",").map(name => name.trim().toLowerCase())]);
        for (let index = 0; index < response.rawHeaders.length; index += 2) {
          const name = response.rawHeaders[index];
          if (!excluded.has(name.toLowerCase())) headers.append(name, response.rawHeaders[index + 1]);
        }
        const bodyless = request.method === "HEAD" || [204, 205, 304].includes(response.statusCode);
        if (bodyless) response.resume();
        resolve(new Response(bodyless ? null : Readable.toWeb(response), {
          status: response.statusCode,
          headers,
        }));
      });
      upstream.once("error", reject);
      if (request.body) {
        const body = Readable.fromWeb(request.body);
        body.once("error", error => upstream.destroy(error));
        upstream.once("close", () => body.destroy());
        body.pipe(upstream);
      } else {
        upstream.end();
      }
    });
  }
  async function websocketTarget(request, clientIp) {
    const url = new URL(request.url);
    const origin = await start(url.origin);
    return { url: `${origin}${url.pathname}${url.search}`, headers: forwardHeaders(request, clientIp) };
  }
  return { fetch, websocketTarget, close: () => { child?.kill(); child = undefined; ready = undefined; } };
}

export function unavailable() {
  return Response.json({ error: { message: "GPROXY is unavailable. Check the deployment database and secrets in the platform console." } }, { status: 503 });
}

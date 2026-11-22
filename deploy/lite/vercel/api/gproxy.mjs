import { createHash, timingSafeEqual } from "node:crypto";
import { readFile } from "node:fs/promises";
import { createGateway, lazily, renewDue } from "../lite/gateway.mjs";
import { upstashRedisStore } from "../lite/stores/upstash-redis.mjs";

const gateway = lazily(async () => createGateway({
  wasm: new WebAssembly.Module(
    await readFile(new URL("../lite/pkg/gproxy_lite_bg.wasm", import.meta.url)),
  ),
  config: process.env.GPROXY_LITE_CONFIG,
  // The Marketplace integration sets the KV_ names; Upstash's own are read too.
  store: upstashRedisStore(
    process.env.KV_REST_API_URL ?? process.env.UPSTASH_REDIS_REST_URL,
    process.env.KV_REST_API_TOKEN ?? process.env.UPSTASH_REDIS_REST_TOKEN,
  ),
}));

const digest = text => createHash("sha256").update(text).digest();

// The Cron Job in vercel.json: Vercel sends `GET` with `CRON_SECRET` as the
// bearer token, and nothing when the variable is not set. Anyone else asks
// the engine with the access key (`POST /lite/renew`).
function isCron(request) {
  const secret = process.env.CRON_SECRET;
  return Boolean(secret)
    && request.method === "GET"
    && new URL(request.url).pathname === "/lite/renew"
    && timingSafeEqual(
      digest(request.headers.get("authorization") ?? ""),
      digest(`Bearer ${secret}`),
    );
}

export default {
  async fetch(request) {
    const started = await gateway();
    if (isCron(request)) return Response.json({ renewals: await renewDue(started) });
    return started instanceof Response ? started : started.fetch(request);
  },
};

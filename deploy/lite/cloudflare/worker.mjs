import wasm from "./lite/pkg/gproxy_lite_bg.wasm";
import { createGateway, lazily, renewDue } from "./lite/gateway.mjs";
import { State, durableObjectStore } from "./lite/stores/durable-object.mjs";

export { State };

// The Worker keeps one end of a pair and hands the engine the other.
async function websocket(request, serve) {
  const [client, server] = Object.values(new WebSocketPair());
  const refused = await serve(request, server);
  return refused ?? new Response(null, { status: 101, webSocket: client });
}

const gateway = lazily(env => createGateway({
  wasm,
  config: env.GPROXY_LITE_CONFIG,
  store: durableObjectStore(env.STATE),
  websocket,
}));

export default {
  async fetch(request, env) {
    const started = await gateway(env);
    return started instanceof Response ? started : started.fetch(request);
  },
  // The Cron Trigger in wrangler.toml.
  async scheduled(controller, env) {
    await renewDue(await gateway(env));
  },
};

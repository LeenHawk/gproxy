// One engine per function instance; the request function and the scheduled
// one each start their own, over the same store.

import { readFile } from "node:fs/promises";
import { createGateway, lazily } from "./lite/gateway.mjs";
import { netlifyBlobsStore } from "./lite/stores/netlify-blobs.mjs";

export const gateway = lazily(async () => createGateway({
  wasm: new WebAssembly.Module(
    await readFile(new URL("./lite/pkg/gproxy_lite_bg.wasm", import.meta.url)),
  ),
  config: process.env.GPROXY_LITE_CONFIG,
  store: netlifyBlobsStore(),
}));

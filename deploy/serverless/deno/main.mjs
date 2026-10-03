import { createGateway, unavailable } from "../lib/server.mjs";
import { denoFetch } from "../lib/deno.mjs";

const gateway = createGateway({ env: Deno.env.toObject() });
Deno.serve(async (request, info) => {
  try {
    return await denoFetch(gateway, request, info.remoteAddr.hostname);
  } catch {
    return unavailable();
  }
});

import { createGateway, unavailable } from "../lib/server.mjs";

const gateway = createGateway({ env: Deno.env.toObject() });
Deno.serve(async (request, info) => {
  try {
    return await gateway.fetch(request, info.remoteAddr.hostname);
  } catch {
    return unavailable();
  }
});

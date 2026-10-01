import { getConnectionString } from "@netlify/database";
import { createGateway, unavailable } from "../../lib/server.mjs";

let gateway;
export default async (request, context) => {
  try {
    gateway ??= createGateway({ env: {
      ...process.env,
      GPROXY_DATABASE_URL: process.env.GPROXY_DATABASE_URL || getConnectionString(),
    } });
    return await gateway.fetch(request, context.ip);
  } catch {
    return unavailable();
  }
};

export const config = { path: "/*", preferStatic: true };

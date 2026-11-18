import { createGateway, unavailable } from "../../lib/server.mjs";

let gateway;
export default async (request, context) => {
  try {
    gateway ??= createGateway();
    return await gateway.fetch(request, context.ip);
  } catch (error) {
    return unavailable(error);
  }
};

export const config = { path: "/*", preferStatic: true };

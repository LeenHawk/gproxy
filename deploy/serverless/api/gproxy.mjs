import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";
import { createGateway, unavailable } from "../lib/server.mjs";

const gateway = createGateway();
export default async function handler(request, response) {
  const controller = new AbortController();
  response.once("close", () => {
    if (!response.writableFinished) controller.abort();
  });
  const url = `https://${request.headers.host}${request.url}`;
  const body = ["GET", "HEAD"].includes(request.method) ? undefined : Readable.toWeb(request);
  const input = new Request(url, { method: request.method, headers: request.headers,
    body, duplex: "half", signal: controller.signal });
  let result;
  try {
    // Vercel overwrites x-forwarded-for at the platform boundary.
    const ip = request.headers["x-forwarded-for"]?.split(",")[0].trim();
    result = await gateway.fetch(input, ip);
  } catch {
    result = unavailable();
  }
  response.statusCode = result.status;
  for (const [name, value] of result.headers) {
    if (name !== "set-cookie") response.setHeader(name, value);
  }
  const cookies = result.headers.getSetCookie();
  if (cookies.length) response.setHeader("set-cookie", cookies);
  if (result.body) await pipeline(Readable.fromWeb(result.body), response);
  else response.end();
}

import { createGateway } from "../lib/server.mjs";
import { createNodeServer } from "../lib/node.mjs";

// Vercel overwrites x-forwarded-for at the platform boundary.
export default createNodeServer(createGateway(), request =>
  request.headers["x-forwarded-for"]?.split(",")[0].trim());

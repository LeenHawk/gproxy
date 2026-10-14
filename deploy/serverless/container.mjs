import { createGateway } from "./lib/server.mjs";
import { createNodeServer } from "./lib/node.mjs";

const gateway = createGateway();
// Use the socket peer; forwarded client-IP headers depend on the platform's
// trusted proxy contract. The gateway still receives the public HTTPS origin.
const server = createNodeServer(gateway, request => request.socket.remoteAddress);
server.listen(Number(process.env.PORT || 8787), "0.0.0.0");

function shutdown() {
  server.close(() => gateway.close());
  // Long-lived SSE/WebSocket connections must not outlive the platform's
  // termination grace period.
  setTimeout(() => {
    gateway.close();
    process.exit(0);
  }, 25_000).unref();
}
process.once("SIGTERM", shutdown);
process.once("SIGINT", shutdown);

// Trivial EdgeOne Pages edge function — feasibility probe.
//
// EdgeOne Pages routes by directory: this file (edge-functions/healthz.js)
// serves GET /healthz. Edge Functions are a V8 / Web-Service-Worker runtime;
// the handler is the exported `onRequest(context)` (NOT addEventListener,
// which Edge Functions do not support).
export function onRequest(context) {
  return new Response("hello-edgeone-pages\n", {
    headers: { "content-type": "text/plain" },
  });
}

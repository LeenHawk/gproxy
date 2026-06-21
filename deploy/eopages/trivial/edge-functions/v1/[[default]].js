// Trivial EdgeOne Pages probe — does a SUBDIR catch-all register on direct
// uploads? (The ROOT edge-functions/[[default]].js was proven to fall back to
// static assets — see ../../NOTES.md Step 4. This nested shape is the open
// question gating the real /v1 gateway routes.)
export function onRequest(context) {
  const url = new URL(context.request.url);
  return new Response(`v1-catchall-ok path=${url.pathname}\n`, {
    headers: { "content-type": "text/plain" },
  });
}

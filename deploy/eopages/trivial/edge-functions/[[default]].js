// Probe: ROOT catch-all — previously (CLI 1.5.6) this never registered and
// every unmatched path fell back to static assets. EdgeOne says the
// catch-all routing bug was fixed in 1.5.9; re-probing with the current CLI.
export function onRequest(context) {
  const url = new URL(context.request.url);
  return new Response(`root-catchall-ok path=${url.pathname}\n`, {
    headers: { "content-type": "text/plain" },
  });
}

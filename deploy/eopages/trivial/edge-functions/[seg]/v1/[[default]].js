// Probe: dynamic dir + nested static dir + catch-all (the shape that failed
// as [provider]/v1/[[default]].js in the previous round — re-test under the
// same dynamic-dir name as the working probes).
export function onRequest(context) {
  const url = new URL(context.request.url);
  const seg = context.params ? context.params.seg : undefined;
  return new Response(`dyn-v1-catchall-ok seg=${seg} path=${url.pathname}\n`, {
    headers: { "content-type": "text/plain" },
  });
}

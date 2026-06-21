// Probe: dynamic dir + DIRECT catch-all — the candidate shape for GPROXY's
// scoped gateway routes (/{provider}/...). If this registers, the wasm can
// receive every scoped path via [provider]/[[default]].js.
export function onRequest(context) {
  const url = new URL(context.request.url);
  const seg = context.params ? context.params.seg : undefined;
  return new Response(`dyn-catchall-ok seg=${seg} path=${url.pathname}\n`, {
    headers: { "content-type": "text/plain" },
  });
}

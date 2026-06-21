// Trivial EdgeOne Pages probe — does ANY dynamic segment register on direct
// uploads? Simplest shape: dynamic dir + static file (/xxx/echo). If this
// fails too, dynamic segments are unsupported in direct uploads entirely
// (not just the nested-catch-all combination).
export function onRequest(context) {
  const url = new URL(context.request.url);
  const seg = context.params ? context.params.seg : undefined;
  return new Response(`dynseg-ok seg=${seg} path=${url.pathname}\n`, {
    headers: { "content-type": "text/plain" },
  });
}

// WASM feasibility probe for EdgeOne Pages Edge Functions.
//
// Instantiates a 41-byte WebAssembly module (exports add(i32,i32)->i32) from a
// base64-embedded byte buffer via WebAssembly.instantiate(bytes, imports) — the
// "runtime byte compilation" path (NOT a static `import x from './m.wasm'`).
// If EdgeOne's V8 isolate exposes the WebAssembly global and allows buffer
// instantiation, GET /wasmtest returns "5"; otherwise it reports the failure.
const WASM_B64 = "AGFzbQEAAAABBwFgAn9/AX8DAgEABwcBA2FkZAAACgkBBwAgACABags=";

export async function onRequest(context) {
  try {
    if (typeof WebAssembly === "undefined") {
      return new Response("NO_WEBASSEMBLY_GLOBAL", { status: 200 });
    }
    const bytes = Uint8Array.from(atob(WASM_B64), (c) => c.charCodeAt(0));
    const m = await WebAssembly.instantiate(bytes, {});
    const r = m.instance.exports.add(2, 3);
    return new Response(String(r), {
      headers: { "content-type": "text/plain" },
    });
  } catch (e) {
    return new Response("WASM_ERROR: " + (e && e.stack ? e.stack : String(e)), {
      status: 200,
      headers: { "content-type": "text/plain" },
    });
  }
}

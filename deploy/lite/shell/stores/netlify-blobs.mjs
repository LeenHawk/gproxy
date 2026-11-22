// Netlify: one blob per key, in a site-wide store read with strong
// consistency.
//
// A write is conditional on the blob's ETag (`onlyIfMatch`) or on its absence
// (`onlyIfNew`), which Netlify Blobs applies atomically, so a write is a real
// compare-and-exchange. Blobs has no conditional delete: a delete is a
// conditional write of a tombstone, and the key stays behind as a few bytes.
// The version is a random id stored in the blob, never reused.

import { getStore } from "@netlify/blobs";

export function netlifyBlobsStore(name = "gproxy-lite") {
  const store = () => getStore({ name, consistency: "strong" });
  const read = key => store().getWithMetadata(key, { type: "json" });
  return {
    async get(key) {
      const blob = await read(key);
      return blob && !blob.data.deleted ? { value: blob.data.value, version: blob.data.version } : null;
    },
    async compareExchange(key, expected, value) {
      const blob = await read(key);
      const live = blob && !blob.data.deleted;
      if ((live ? blob.data.version : null) !== expected) return null;
      // Without an ETag the write below would not be conditional at all.
      if (blob && !blob.etag) throw new Error("Netlify Blobs returned no ETag for an existing key");
      const version = crypto.randomUUID();
      const body = value === null ? { version, deleted: true } : { version, value };
      const condition = blob ? { onlyIfMatch: blob.etag } : { onlyIfNew: true };
      const { modified } = await store().setJSON(key, body, condition);
      if (!modified) return null;
      return { version: value === null ? null : version };
    },
  };
}

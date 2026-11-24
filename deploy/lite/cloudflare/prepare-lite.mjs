// Fetches the prebuilt gproxy-lite bundle into ./lite, so a deployment needs
// no Rust toolchain. It is the build step of the deploy buttons.
//
// GPROXY_LITE_RELEASE picks the release: `staging` (the default, built from
// main), `latest` (the latest stable release) or a tag such as `v4.3.1`.
// A ./lite that is already there, from deploy/lite/build.sh or an earlier
// run, is kept; delete it to fetch again.
//
// The same file is in every template of deploy/lite; keep them identical.

import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import { mkdir, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { inflateRawSync } from "node:zlib";

const root = dirname(fileURLToPath(import.meta.url));
const release = process.env.GPROXY_LITE_RELEASE || "staging";
const base = release === "latest"
  ? "https://github.com/LeenHawk/gproxy/releases/latest/download"
  : `https://github.com/LeenHawk/gproxy/releases/download/${release}`;
const name = "gproxy-lite.zip";

if (existsSync(join(root, "lite/pkg/gproxy_lite_bg.wasm"))) {
  console.log("lite/ is already there; delete it to fetch the bundle again.");
  process.exit(0);
}

async function download(file) {
  const response = await fetch(`${base}/${file}`);
  if (!response.ok) throw new Error(`${base}/${file}: HTTP ${response.status}`);
  return Buffer.from(await response.arrayBuffer());
}

const [archive, sums] = await Promise.all([download(name), download("SHA256SUMS")]);
const expected = sums.toString().split(/\r?\n/)
  .map(line => line.trim().split(/\s+/))
  .find(([, file]) => file?.replace(/^\*/, "") === name)?.[0];
if (!expected) throw new Error(`${name} is not in the release's SHA256SUMS`);
if (createHash("sha256").update(archive).digest("hex") !== expected) {
  // The release is replaced while main publishes; a retry a few minutes later gets a consistent pair.
  throw new Error(`${name}: SHA-256 does not match the release's SHA256SUMS`);
}

// The bundle is a plain deflate ZIP written by scripts/reproducible-archive.py.
const end = archive.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
if (end < 0) throw new Error(`${name}: not a ZIP archive`);
let entry = archive.readUInt32LE(end + 16);
for (let count = archive.readUInt16LE(end + 10); count > 0; count--) {
  const method = archive.readUInt16LE(entry + 10);
  const size = archive.readUInt32LE(entry + 20);
  const nameLength = archive.readUInt16LE(entry + 28);
  const skip = nameLength + archive.readUInt16LE(entry + 30) + archive.readUInt16LE(entry + 32);
  const local = archive.readUInt32LE(entry + 42);
  const path = archive.toString("utf8", entry + 46, entry + 46 + nameLength);
  entry += 46 + skip;
  if (!path.startsWith("lite/") || path.split("/").includes("..")) {
    throw new Error(`${name}: unexpected entry ${path}`);
  }
  if (path.endsWith("/")) continue;
  const start = local + 30 + archive.readUInt16LE(local + 26) + archive.readUInt16LE(local + 28);
  const data = archive.subarray(start, start + size);
  const output = join(root, path);
  await mkdir(dirname(output), { recursive: true });
  await writeFile(output, method === 8 ? inflateRawSync(data) : data);
}
console.log(`Fetched gproxy-lite from ${base} (SHA-256 verified).`);

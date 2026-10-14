import { createHash } from "node:crypto";
import { mkdir, writeFile } from "node:fs/promises";
import { dirname, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { unzipSync } from "fflate";

const version = process.env.GPROXY_RELEASE_VERSION || "latest";
const asset = "gproxy-edge-cloudflare.zip";
const base = version === "latest"
  ? "https://github.com/LeenHawk/gproxy/releases/latest/download"
  : `https://github.com/LeenHawk/gproxy/releases/download/${version}`;
const root = fileURLToPath(new URL(".", import.meta.url));

async function download(name) {
  const response = await fetch(`${base}/${name}`);
  if (!response.ok) throw new Error(`${name}: HTTP ${response.status}`);
  return Buffer.from(await response.arrayBuffer());
}

const [archive, checksums] = await Promise.all([download(asset), download("SHA256SUMS")]);
const expected = checksums.toString().split(/\r?\n/)
  .map(line => line.trim().split(/\s+/))
  .find(([, name]) => name?.replace(/^\*/, "") === asset)?.[0];
if (createHash("sha256").update(archive).digest("hex") !== expected) {
  throw new Error("Release archive SHA-256 mismatch");
}

// Keep the template's Wrangler config: the deploy form sets its D1 database ID.
const entries = unzipSync(archive);
for (const [name, bytes] of Object.entries(entries)) {
  if (!/^cloudflare\/(build|public)\//.test(name) || name.endsWith("/")) continue;
  const target = resolve(root, name.slice("cloudflare/".length));
  if (!target.startsWith(root.endsWith(sep) ? root : root + sep)) throw new Error("Invalid archive path");
  await mkdir(dirname(target), { recursive: true });
  await writeFile(target, bytes);
}
console.log(`Prepared GPROXY ${version} Worker and console (SHA-256 verified).`);

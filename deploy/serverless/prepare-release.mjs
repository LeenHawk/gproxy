import { createHash } from "node:crypto";
import { chmod, mkdir, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { unzipSync } from "fflate";

// Upgrade by changing this tag in the cloned template and redeploying.
const version = "v4.0.1";
const base = `https://github.com/LeenHawk/gproxy/releases/download/${version}`;
const root = new URL(".", import.meta.url);

async function download(name) {
  const response = await fetch(`${base}/${name}`);
  if (!response.ok) throw new Error(`${name}: HTTP ${response.status}`);
  return Buffer.from(await response.arrayBuffer());
}

const checksums = (await download("SHA256SUMS")).toString().split(/\r?\n/)
  .map(line => line.trim().split(/\s+/));
// Function build and runtime CPUs need not match. Both static binaries fit
// within function bundle limits; choose the matching one at runtime.
for (const [arch, target] of [["x64", "x86_64"], ["arm64", "aarch64"]]) {
  const name = `gproxy-serverless-linux-${target}-musl.zip`;
  const archive = await download(name);
  const expected = checksums.find(([, file]) => file?.replace(/^\*/, "") === name)?.[0];
  if (createHash("sha256").update(archive).digest("hex") !== expected) {
    throw new Error(`${name}: release SHA-256 mismatch`);
  }
  const binary = unzipSync(archive)["gproxy-serverless"];
  if (!binary) throw new Error(`${name}: missing serverless executable`);
  const directory = new URL(`.gproxy/${arch}/`, root);
  await mkdir(directory, { recursive: true });
  const output = new URL("gproxy-serverless", directory);
  await writeFile(output, binary);
  await chmod(output, 0o755);
}
await mkdir(new URL("public/", root), { recursive: true });
await writeFile(new URL("public/index.html", root),
  '<!doctype html><meta charset="utf-8"><meta http-equiv="refresh" content="0;url=/console/"><a href="/console/">Open GPROXY</a>\n');
console.log(`Prepared GPROXY ${version} serverless binaries in ${fileURLToPath(root)} (SHA-256 verified).`);

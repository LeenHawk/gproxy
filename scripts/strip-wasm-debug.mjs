import { readFileSync, writeFileSync } from 'node:fs';

// Run after worker-build: wasm-bindgen needs target_features during the build,
// and worker-build itself asks wasm-opt to retain the names section.
const files = process.argv.slice(2);
if (files.length === 0) throw new Error('Usage: node strip-wasm-debug.mjs <file.wasm> [...]');

for (const file of files) {
  const wasm = readFileSync(file);
  if (!wasm.subarray(0, 8).equals(Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]))) {
    throw new Error(`Invalid WASM header: ${file}`);
  }
  let offset = 8;
  function readU32() {
    let value = 0;
    for (let shift = 0; shift < 35; shift += 7) {
      if (offset >= wasm.length) throw new Error(`Truncated WASM: ${file}`);
      const byte = wasm[offset++];
      value += (byte & 127) * 2 ** shift;
      if (byte < 128 && value <= 0xffffffff) return value;
    }
    throw new Error(`Invalid WASM section length: ${file}`);
  }
  const sections = [wasm.subarray(0, 8)];
  while (offset < wasm.length) {
    const start = offset;
    const id = wasm[offset++];
    const length = readU32();
    const end = offset + length;
    if (end > wasm.length) throw new Error(`Truncated WASM section: ${file}`);
    let debug = false;
    if (id === 0) {
      const nameLength = readU32();
      if (offset + nameLength > end) throw new Error(`Invalid WASM custom section: ${file}`);
      const name = wasm.toString('utf8', offset, offset + nameLength);
      debug = name === 'name' || name.startsWith('.debug_');
    }
    if (!debug) sections.push(wasm.subarray(start, end));
    offset = end;
  }
  const stripped = Buffer.concat(sections);
  writeFileSync(file, stripped);
  console.log(`${file}: stripped ${wasm.length - stripped.length} debug bytes (${stripped.length} bytes remaining)`);
}

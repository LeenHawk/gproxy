#!/usr/bin/env node
// i18n parity checker — every locale must carry the SAME key set as `en` for
// every namespace. Exits non-zero (listing the drift) on any missing/extra key.
// Run from console/: `node scripts/i18n-parity.mjs`
import { readdirSync, readFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const localesDir = join(dirname(fileURLToPath(import.meta.url)), "..", "src", "locales");
const REFERENCE = "en";

const flat = (obj, prefix = "") =>
  Object.entries(obj).flatMap(([k, v]) =>
    v && typeof v === "object" && !Array.isArray(v) ? flat(v, `${prefix}${k}.`) : [`${prefix}${k}`],
  );

const locales = readdirSync(localesDir, { withFileTypes: true })
  .filter((d) => d.isDirectory())
  .map((d) => d.name);

if (!locales.includes(REFERENCE)) {
  console.error(`i18n-parity: reference locale "${REFERENCE}" not found in ${localesDir}`);
  process.exit(1);
}

const namespaces = readdirSync(join(localesDir, REFERENCE))
  .filter((f) => f.endsWith(".json"))
  .map((f) => f.replace(/\.json$/, ""));

let failed = false;
for (const ns of namespaces) {
  const ref = new Set(flat(JSON.parse(readFileSync(join(localesDir, REFERENCE, `${ns}.json`)))));
  for (const loc of locales) {
    if (loc === REFERENCE) continue;
    let keys;
    try {
      keys = new Set(flat(JSON.parse(readFileSync(join(localesDir, loc, `${ns}.json`)))));
    } catch {
      console.error(`✗ ${loc}/${ns}.json — missing or unparseable`);
      failed = true;
      continue;
    }
    const missing = [...ref].filter((k) => !keys.has(k));
    const extra = [...keys].filter((k) => !ref.has(k));
    if (missing.length || extra.length) {
      failed = true;
      console.error(`✗ ${loc}/${ns}.json`);
      if (missing.length) console.error(`    missing: ${missing.join(", ")}`);
      if (extra.length) console.error(`    extra:   ${extra.join(", ")}`);
    }
  }
}

if (failed) {
  console.error("\ni18n parity check FAILED");
  process.exit(1);
}
console.log(`i18n parity OK — ${namespaces.length} namespaces × ${locales.length} locales aligned`);

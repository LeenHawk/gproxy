import { copyFile, mkdir, readdir, rm, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";

const docsDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const sourceDir = path.join(docsDir, "release-notes");
const outputDir = path.join(docsDir, "public", "release-notes");

await rm(outputDir, { recursive: true, force: true });
await mkdir(outputDir, { recursive: true });

const files = (await readdir(sourceDir)).filter((name) => name.endsWith(".md"));
await Promise.all(files.map((name) => copyFile(path.join(sourceDir, name), path.join(outputDir, name))));

// Only versioned, stable releases belong in the cumulative-update index.
// Pre-release and arbitrary Markdown files may still be published, but a
// production binary must never offer them as an upgrade destination.
const stableVersion = /^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)\.md$/;
const versions = files
  .map((name) => stableVersion.exec(name))
  .filter((match) => match !== null)
  .map((match) => ({
    version: match.slice(1).join("."),
    parts: match.slice(1).map(BigInt),
  }))
  .sort((left, right) => {
    for (let index = 0; index < left.parts.length; index += 1) {
      if (left.parts[index] > right.parts[index]) return -1;
      if (left.parts[index] < right.parts[index]) return 1;
    }
    return 0;
  })
  .map(({ version }) => version);

await writeFile(
  path.join(outputDir, "index.json"),
  `${JSON.stringify({ versions }, null, 2)}\n`,
  "utf8",
);

console.log(
  `Copied ${files.length} release-note files and indexed ${versions.length} stable releases.`,
);

import { copyFile, mkdir, readdir, rm } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";

const docsDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const sourceDir = path.join(docsDir, "release-notes");
const outputDir = path.join(docsDir, "public", "release-notes");

await rm(outputDir, { recursive: true, force: true });
await mkdir(outputDir, { recursive: true });

const files = (await readdir(sourceDir)).filter((name) => name.endsWith(".md"));
await Promise.all(files.map((name) => copyFile(path.join(sourceDir, name), path.join(outputDir, name))));
console.log(`Copied ${files.length} release-note files to public/release-notes.`);

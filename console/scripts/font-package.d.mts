export interface FontFile {
  /** `<sha256>.woff2`, the name the stylesheet uses. */
  name: string
  /** The npm package that ships it, e.g. `@fontsource-variable/noto-sans`. */
  package: string
  /** Its path inside that package, e.g. `files/noto-sans-latin-wght-normal.woff2`. */
  path: string
  /** Where it is on disk in this checkout. */
  local: string
}

export interface FontPackage {
  stylesheet: string
  css: string
  packages: Record<string, string>
  files: FontFile[]
  licenses: { fileName: string, source: Buffer }[]
}

export function fontPackage(consoleDir: string): FontPackage

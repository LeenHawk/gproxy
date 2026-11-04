# Console fonts

The Console ships a stylesheet, a manifest and font licenses, but no WOFF2
binaries. It uses system fonts by default. The first-run wizard and Settings
offer an optional download; only that explicit action contacts the network.

## Where the font pack comes from

`scripts/font-package.mjs` builds the pack from the four pinned
`@fontsource-variable/*` devDependencies in `package.json`. The stylesheet
names every file by its SHA-256. The manifest (`fonts/manifest.json` in the
build) records, for each file, that name plus the npm package and path that
ship it:

```json
{ "name": "<sha256>.woff2", "package": "@fontsource-variable/noto-sans", "path": "files/noto-sans-latin-wght-normal.woff2" }
```

Nothing is stored in this repository: `vite build` reads the installed
packages, and so does `scripts/prepare-edge-fonts.mjs`.

## How hosts get the files

- **Native hosts** download each file from public npm CDNs, trying
  npmmirror, then jsDelivr, then unpkg
  (`crates/gproxy-host-axum/src/console/fonts.rs`). A file is accepted only if
  its SHA-256 matches its name, so the CDN is not trusted. Files are saved in
  the local `fonts/` directory; a complete download activates the stylesheet.
  Status checks, startup and font reads never download anything. The pack works
  offline until the user deletes it in Settings; neither an upgrade nor old
  cached files opt users in.
- **Edge** copies the files into the site's static assets and serves them from
  the same origin; it does not use a CDN.

Application fonts belong to the application's data directory, independently of
a custom gateway instance directory. CLI fonts use the instance data directory.
Headless packages exclude the Console and its font metadata.

Clients released before the CDN switch download from
`https://gproxy.leenhawk.com/fonts/<sha256>.woff2`. The documentation site
keeps those files, frozen, for them; nothing new is added there.

## Updating

Change the pinned versions in `package.json` and install. The next build picks
them up. The exact versions must exist on npm, which is what the CDNs mirror.

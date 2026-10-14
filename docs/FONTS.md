# Console font distribution

Cloudflare Pages serves `public/fonts/` at `https://gproxy.leenhawk.com/fonts/`.
The files are named by SHA-256. `_headers` permits cross-origin reads and gives
immutable font files a one-year cache lifetime; `manifest.json` is not cached.
Font licenses are hosted alongside the files and included in application packages.

CLI and Application packages contain the stylesheet, a manifest, and licenses,
but no WOFF2 binaries. They use system fonts by default. The first-run wizard and
Settings offer an optional download. Only that explicit action contacts the CDN.
GPROXY verifies each font's SHA-256 and saves it in its local `fonts/` directory.
A complete download activates the local stylesheet. Status checks, startup, and
font reads never download anything, including when files are missing or corrupt.
The installed pack works offline until the user deletes it in Settings. Deleting
it restores system fonts; neither an upgrade nor old cached files opt users in.

Application fonts belong to the application's own data directory, independently
of a custom gateway instance directory. CLI fonts use the instance data directory.
Headless packages exclude the Console and its font metadata. Edge deployment
copies fonts into the site's static assets and serves them from the same origin;
it does not add font files to a native executable or require an external font CDN.

To update the fonts, change the four pinned Fontsource development dependencies
in `console/package.json`, install them, and run:

```sh
pnpm --dir console fonts:update
```

Commit the generated files and `manifest.json`. **Keep previous hash-named files
and licenses**: installed versions can still request those files when a user
chooses to download fonts. The generator adds files without deleting old versions.
Deploy the documentation site before publishing a client that references new
fonts. No separate Worker, bucket or Cloudflare Fonts dashboard switch is needed.

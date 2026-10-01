# Console font distribution

Cloudflare Pages serves `public/fonts/` at `https://gproxy.leenhawk.com/fonts/`.
The font files and stylesheets are named by SHA-256. `_headers` permits
cross-origin font loading and gives these immutable files a one-year cache
lifetime. The source licenses are included alongside them.

The Console build embeds only the stylesheet named in `manifest.json`.
CLI and Application hosts fetch missing WOFF2 files into their local `fonts/`
cache when the browser needs a face, verify its hash, and atomically save it.
The Edge packaging step points the stylesheet directly at the documentation
site; browsers then fetch its relative font URLs from the same CDN.

To update the fonts, change the four pinned Fontsource development dependencies
in `console/package.json`, install them, and run:

```sh
pnpm --dir console fonts:update
```

Commit the new files and `manifest.json`. **Keep all previous hash-named files
and licenses**: installed releases still refer to those URLs. The generator
adds files without deleting old versions. Deploy the documentation site before
releasing clients that refer to newly generated fonts. No Cloudflare Fonts
dashboard switch, Worker, or separate bucket is needed.

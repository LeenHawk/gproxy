# Console font distribution

Cloudflare Pages serves `public/fonts/` at `https://gproxy.leenhawk.com/fonts/`.
The font files and stylesheets are named by SHA-256. `_headers` permits
cross-origin font loading and gives these immutable files a one-year cache
lifetime. The source licenses are included alongside them.

The Console build bundles the stylesheet named in `manifest.json`, all WOFF2
subsets referenced by it, and the font licenses. CLI and Application serve
these assets locally; Edge deploys them with the Console as same-origin static
assets. Headless packages do not embed the Console or fonts. No runtime font
download cache or external font service is used by new builds.

To update the fonts, change the four pinned Fontsource development dependencies
in `console/package.json`, install them, and run:

```sh
pnpm --dir console fonts:update
```

Commit the new files and `manifest.json`. **Keep all previous hash-named files
and licenses**: installed releases still refer to those URLs. The generator
adds files without deleting old versions. Keep deploying the documentation fonts for older clients that still fetch them. No Cloudflare Fonts
dashboard switch, Worker, or separate bucket is needed.

# Application icons

`icon.png` is the existing project globe icon copied from
`docs/public/web-app-manifest-512x512.png`. Do not replace it with Tauri's
placeholder image. The PNGs used by the Console are the same globe design;
`docs/public/favicon.svg` contains an older illustration.

Generate platform variants with the pinned CLI:

```sh
pnpm --dir crates/gproxy-host-tauri exec tauri icon icons/icon.png --output /tmp/gproxy-icons
```

Copy `icon.ico`, `icon.icns`, `StoreLogo.png`, `Square44x44Logo.png`, and
`Square150x150Logo.png` into this directory. Copy the generated Android launcher
PNGs into the matching existing `gen/android/app/src/main/res/mipmap-*` paths.

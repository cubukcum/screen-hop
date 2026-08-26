# screen-hop app icon assets

`app-icon-source.png` is the original 1254×1254 artwork supplied for the app icon. The derived
assets remove only near-invisible alpha noise, preserve the original square composition, and use
premultiplied-alpha Lanczos resizing.

- `app-icon-1024.png`: cleaned high-resolution app artwork.
- `window-icon-256.png` and `window-icon-64.png`: Slint/native window icon assets.
- `screen-hop.ico`: Windows icon resource with 16, 20, 24, 32, 40, 48, 64, 128, and 256 px frames.

The source PNG is already large enough for these outputs. An SVG of the same detailed artwork is
not required; only a separately simplified small-size design would materially improve 16–24 px.

# coldfront

Chill weather apps. Enter a US ZIP code and Coldfront shows current conditions, the next 24 hours, a 10-day forecast, live NEXRAD radar, and active National Weather Service alerts.

```
apps/coldfront-web      Vite + React app, deployed as a Cloudflare Worker
apps/cosmic-coldfront   COSMIC panel applet (libcosmic / iced)
crates/coldfront-core   Rust domain logic the applet calls
fixtures/               JSON samples and contract.json shared by both test suites
arch/                   Arch PKGBUILD for the applet
```

The web app and the applet do not share a binary. Each implements the same rules, and both test suites assert `fixtures/contract.json`, so labels, rounding, and radar tile math cannot drift quietly.

## Data sources

| Source | Used for | Key |
| --- | --- | --- |
| [Zippopotam.us](https://api.zippopotam.us) | ZIP to city and coordinates | none |
| [Open-Meteo](https://open-meteo.com/) | current, hourly, and daily forecast | none |
| [api.weather.gov](https://www.weather.gov/documentation/services-web-api) | active alerts | none; a User-Agent is required |
| [Iowa Environmental Mesonet](https://mesonet.agron.iastate.edu/GIS/ridge.phtml) | NEXRAD base-reflectivity tiles | none |
| [MapTiler](https://www.maptiler.com/) | raster street basemap | optional |

Forecasts are requested in Celsius and km/h and converted for display, so switching °F / °C does not refetch. Half-away-from-zero rounding is what both clients use (`-1.5` becomes `-2`).

The User-Agent on every upstream request is `Coldfront/0.1 (https://github.com/drmaas/coldfront)`.

## Web app

The site is a Vite + React single-page app on **Cloudflare Workers static assets**, with a Worker in front of `/api/*`. That is the current Vite plugin layout, not Cloudflare Pages and not the deprecated Workers Sites. The Worker exists so the browser never holds the MapTiler key and so NWS receives a User-Agent a browser is not allowed to set.

```bash
cd apps/coldfront-web
cp .dev.vars.example .dev.vars   # optional: set MAPTILER_API_KEY
npm ci
npm run dev                      # http://localhost:5173
npm test
npm run lint
npm run typecheck
npm run build
```

Routes are `/` and `/:zip` (for example `/55115`). Units live in `localStorage` under `coldfront-units`.

The radar card is a canvas. It stacks MapTiler raster tiles (proxied at `/api/map/{z}/{x}/{y}.png`) under IEM NEXRAD tiles, draws a pin on the ZIP, and loops twelve frames from 55 minutes ago to the latest scan. Drag to pan. Zoom stays between 5 and 11. There is no MapLibre or other WebGL map: the same raster tiles draw on a 2D canvas, so the card still works where WebGL is missing. While the first frame is in flight the card shows “Loading radar…”.

`/api/config` returns `{ "basemap": "maptiler" }` or `{ "basemap": "none" }`. It never returns the key. With no key the card is radar on a plain background, plus the pin and the loop.

### Deploy (manual)

This repository does not deploy itself. From `apps/coldfront-web`, after `npx wrangler login`:

```bash
npx wrangler secret put MAPTILER_API_KEY   # optional; skip it and the map has no streets
npm run build && npx wrangler deploy
```

`MAPTILER_API_KEY` is intentionally not in `secrets.required`. A missing secret must not fail the deploy; the site falls back to radar without a basemap. Do not put the key in `wrangler.jsonc`, source, or `.dev.vars` that you commit. `.dev.vars` is gitignored. After changing bindings, regenerate the checked-in types:

```bash
npx wrangler types --include-runtime false
```

`compatibility_date` is `2026-11-17`. Workers Logs are enabled and traces are sampled at 1%.

## COSMIC applet

`cosmic-coldfront` is a panel applet. The popup opens with a one-line summary and, immediately under it, the radar map. The map is not behind a tab or a button. Settings (ZIP, units, MapTiler key, OSM opt-in) sit below the forecast.

libcosmic has no map widget. The applet downloads a 3×3 window of raster tiles at zoom 7 around the saved ZIP, composites them in `coldfront-core` (basemap, then IEM radar, then a pin), and shows the result with iced's image widget. The first thing drawn is the latest scan, with “Loading radar map…” until those bytes arrive. Six frames, ten minutes apart, then loop while the popup is open. Pause is under the image. Reopening the popup shows the last composite immediately and refreshes in the background only when the forecast or frames are missing.

Basemap choice, in order:

1. `MAPTILER_API_KEY` in the environment, if it is non-empty.
2. The key saved in cosmic-config.
3. OpenStreetMap raster tiles, only when “OSM basemap” is switched on.
4. Otherwise radar and the pin on a plain background.

The OSM public tile servers are not a fallback for a missing MapTiler key. The [OpenStreetMap tile usage policy](https://operations.osmfoundation.org/policies/tiles/) asks applications not to use those tiles heavily or from distributed installs, and to send a valid User-Agent and attribution when use is acceptable. Coldfront's User-Agent is sent, and the toggle defaults to off. Turn it on only for a personal machine.

```bash
# from the repository root, with the COSMIC build dependencies installed
just applet build-release
just applet install          # /usr/bin/cosmic-coldfront and the desktop file
just applet run
```

The desktop entry, metainfo, and icon are static files, the same way `cosmic-printers` ships them, so `just install` does not run xdgen. There is no `just tag` recipe. The panel icon uses the system icon theme (`weather-*-symbolic`).

Packaging follows the applet crate: `arch/PKGBUILD` builds `-p cosmic-coldfront` out of a `coldfront-$pkgver` archive. `.github/workflows/release-arch.yml` runs only on a `v*` tag. `scripts/check-version.sh vX.Y.Z` checks that tag, `apps/cosmic-coldfront/Cargo.toml`, and `pkgver` agree. Do not create a tag just to try the workflow.

## Development

```bash
just test     # cargo test and the web unit tests
just check    # fmt, clippy -D warnings, tests, applet build, web lint/typecheck/test/build
```

Rust MSRV is 1.93 because current libcosmic requires it. CI uses stable. `cargo fmt` line width is 100.

Web domain code lives in `apps/coldfront-web/lib`. The Worker proxy lives in `lib/server` and is unit-tested without booting workerd; Vitest does not load the Cloudflare Vite plugin.

The license is GPL-3.0-only, matching libcosmic. Crates are `publish = false`.

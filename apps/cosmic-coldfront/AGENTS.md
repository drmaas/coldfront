# AGENTS.md

Rules for `apps/cosmic-coldfront`. Also follow the repository [AGENTS.md](../../AGENTS.md) and [docs/architecture.md](../../docs/architecture.md).

## When to use

Use this file when you edit the panel applet, its Fluent strings, or its installed desktop files.

## When not to use

Do not put shared parsers or tile math in this crate. Put them in `coldfront-core`, and mirror them in `apps/coldfront-web/lib/domain.ts` when the contract covers them.

Do not call the web Worker from the applet.

## Rules

- Popup tabs are Current, Radar, Hourly, and 10-day. Refresh, Settings, and Alerts open from the header refresh, gear, and alert buttons (not tab-bar segments). Radar is a tab, not stacked above the facts.
- Keep the popup at 480×440 for every tab. Radar may expand toward the monitor and restore to that size; leaving Radar or closing the popup clears expand.
- Popup fill stays opaque. `init` clears applet auto-blur so the frosted palette stays off. The panel button window stays transparent.
- Hourly draws a 24-hour temperature strip with `chart_points`. The 10-day tab draws high/low range bars with `range_placement`. Forecast icons in the popup can be inline SVGs; the panel icon stays `symbolic_icon`.
- `init` must not fetch. After startup, a valid saved ZIP loads place, forecast, and alerts so the panel icon matches. Radar tiles load only when the Radar tab is open (or was opened and needs a reload).
- Call `HttpClient` for ZIP, forecast, and alerts. Call `mapload` for tiles. Call `compose_viewport` for Flat Radar and `compose_relief_layers` plus `render_relief` for the session 3D toggle. Map view size follows the image widget (compact or expanded).
- Load the latest radar frame (offset 0) before older frames. Default offsets are 50, 40, 30, 20, 10, 0. Timeline lookback (1h / 2h / 3h) and speed next to Play (1× / 2× / 4× of 800 ms) are session-only; applet restart resets them. Freeze the frame list (and scan instants) when lookback changes or the map reloads. Growing lookback batch-fetches missing offsets; playback waits on pending frames and skips zero-tile offsets. Keep lookback and speed on one on-map timeline row so the 480×440 popup still fits. Interactive zoom is 5–11. Playback runs while the popup is open on the Radar tab with at least two ready frames. The map clock follows the shown frame’s even-minute instant. Keep the pan/zoom hint chip (`radar-hint` / `radar-hint-relief`) on the Radar stage until the first map gesture clears it; it sits under the on-map clock so it costs no column height.
- During pan/zoom, recompose from the tile cache at the live camera at most once per 16 ms frame, at one device pixel per logical pixel, and inside a 0.5 MP budget (larger views render smaller and the canvas scales them up). Ctrl plus scroll zoom goes through that same budgeted paint as pan; never compose per scroll event. `board_transform` slides the last frame between paints, so pointer motion stays immediate; settle repaints at the display pixel ratio. `snapshot_transform` is only a fallback when cache paint fails. After a quiet settle (50 ms), fetch missing tiles and present once; do not overwrite a good snapshot with pale holes on settle. Denoise each radar tile once at fetch into the tile LRU. Prefetch neighbor zooms and the next frame after a successful present.
- Ignore applying `WeatherReady`, `MapReady`, `OlderFrame`, and `FrameSkipped` when `generation` / `map_generation` does not match. Stale `MapReady` / `OlderFrame` still merge successful tiles into the cache.
- A failed alert request becomes an empty list. Do not fail the forecast for that. A failed weather refresh keeps the last forecast and sets `refresh_failed`.
- Do not refetch when the user switches °F and °C.
- Refetch weather when the saved ZIP changes. Reload the map when the MapTiler key or OSM toggle changes after the Radar tab has been opened.
- Basemap order is environment `MAPTILER_API_KEY`, then the saved key, then OSM if `allow_osm_basemap` is true, otherwise none.
- Do not turn OSM on by default. Do not log tile URLs.
- `Config` is cosmic-config version 1. A new field or a new meaning requires a new `#[version]`.
- User-facing sentences go in `i18n/en/cosmic_coldfront.ftl` and through `fl!`.
- Panel icons come from `symbolic_icon`. Do not ship a custom weather icon set for the panel.
- Desktop file, metainfo, and SVG stay static under `resources/`. Do not generate them in `just install`.
- A release tag must match this crate's `version` and `arch/PKGBUILD` `pkgver`. Do not push a `v*` tag to test that.

## Commands

From the repository root:

```bash
just applet check
just applet test
just applet build-release
cargo test -p coldfront-core -p cosmic-coldfront --locked
```

Local install and uninstall write under `/usr`. Build first, then skip the cargo dependency:

```bash
sudo just --justfile apps/cosmic-coldfront/justfile --no-deps install
sudo just --justfile apps/cosmic-coldfront/justfile --no-deps uninstall
```

`--no-deps` skips `build-release`. Root does not have `cargo` on `PATH`.

## Failures

- Invalid ZIP: set `map_error` from `Error::InvalidZip` and do not call the network.
- ZIP or forecast failure with no prior forecast: `WeatherFailed` with that generation. Leave the previous frames cleared when the ZIP changed.
- Older-frame failure: `FrameSkipped`, then load the next offset.
- `HttpClient` errors must keep omitting the request URL.

## Example

Good: add a settings field on `Config`, bump `#[version]`, and reload only if the new field changes the map.

Bad: fetch OpenStreetMap tiles whenever the MapTiler key is empty.

# AGENTS.md

Rules for `apps/cosmic-coldfront`. Also follow the repository [AGENTS.md](../../AGENTS.md) and [docs/architecture.md](../../docs/architecture.md).

## When to use

Use this file when you edit the panel applet, its Fluent strings, or its installed desktop files.

## When not to use

Do not put shared parsers or tile math in this crate. Put them in `coldfront-core`, and mirror them in `apps/coldfront-web/lib/domain.ts` when the contract covers them.

Do not call the web Worker from the applet.

## Rules

- Keep popup order: summary, radar, current facts, alerts, settings. Do not put the radar behind a tab or a button.
- `init` must not fetch. Fetch on popup open only when `forecast` is missing or `frames` is empty.
- Call `HttpClient` for ZIP, forecast, and alerts. Call `mapload` for tiles. Call `compose_radar_map` to build the image.
- Load the latest radar frame (offset 0) before older frames. Offsets stay 50, 40, 30, 20, 10, 0. Zoom stays 7. The grid stays 3 by 3.
- Ignore `WeatherReady`, `OlderFrame`, and `FrameSkipped` when `generation` does not match.
- A failed alert request becomes an empty list. Do not fail the forecast for that.
- Do not refetch when the user switches °F and °C.
- Refetch when the saved ZIP changes, the MapTiler key changes, or the OSM toggle changes while the popup is open.
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

## Failures

- Invalid ZIP: set `map_error` from `Error::InvalidZip` and do not call the network.
- ZIP or forecast failure: `WeatherFailed` with that generation. Leave the previous frames cleared.
- Older-frame failure: `FrameSkipped`, then load the next offset.
- `HttpClient` errors must keep omitting the request URL.

## Example

Good: add a settings field on `Config`, bump `#[version]`, and reload only if the new field changes the map.

Bad: fetch OpenStreetMap tiles whenever the MapTiler key is empty.

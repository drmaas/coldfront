# Architecture

This file is the design record for Coldfront. If this file and the code disagree, fix the code or update this file in the same change. Setup and release steps live in [CONTRIBUTING.md](../CONTRIBUTING.md).

Coldfront is two clients and no shared backend. A person enters a US ZIP code. Each client resolves that ZIP, loads a forecast and active alerts, and draws NEXRAD radar. Neither client accounts for users, stores a forecast on a server, or calls the other client.

## Two clients

`apps/coldfront-web` is a Vite and React single-page app. Cloudflare Workers static assets serve the built files. A Worker handles `/api/*` first (`run_worker_first`). This is not Cloudflare Pages and not Workers Sites.

`apps/cosmic-coldfront` is a COSMIC panel applet. It links `crates/coldfront-core` and talks to upstream hosts itself. libcosmic has no map widget, so the applet composites raster tiles into one image.

`crates/coldfront-core` is the Rust domain crate. The web app does not call it. `apps/coldfront-web/lib/domain.ts` is the parallel implementation. `fixtures/contract.json` is the lock that keeps them aligned.

The web page shows current conditions, the next 24 hours, a 10-day list, radar, and up to three alerts. The applet popup shows a one-line summary, the radar image, current facts, up to three alerts, and settings. The applet parses the full 10-day forecast and does not draw the hourly chart or the daily list.

## Shared rules

Change a shared rule in Rust, in TypeScript, and in `fixtures/contract.json` in one change. The table shows which rules the contract locks.

| Rule | Where it lives | Locked by the contract |
| --- | --- | --- |
| Five-digit ZIP, leading zeros kept | `is_us_zip` / `isUsZip` | yes |
| Zippopotam parse | `parse_zippopotam` / `parseZippopotam` | yes |
| Open-Meteo parse | `parse_open_meteo` / `parseOpenMeteo` | yes |
| NWS alert parse | `parse_alerts` / `parseAlerts` | yes |
| Half-away-from-zero rounding | `round_half_away_from_zero` / `roundHalfAwayFromZero` | yes |
| Temperature and wind display | `display_temperature`, `display_wind_mph` and the TypeScript twins | yes |
| "As of", hour labels, day labels | `format_as_of`, `hour_label`, `day_label` and the TypeScript twins | yes |
| WMO code to label and icon id | `describe` | sample codes |
| Web Mercator tile math | `lat_lon_to_tile`, `tile_window` | yes |
| IEM tile URL and radar frame lists | `iem_tile_url`, `web_radar_frames`, `applet_radar_frames` | yes |
| HTTP to upstream hosts | `HttpClient` or the Worker | no |
| Painted map | `compose_radar_map` or `RadarMap.tsx` | no |
| Hourly chart and daily range bar | `chartPoints`, `rangePlacement` | no |
| Panel icon names | `symbolic_icon` | no |

`chartPoints` and `rangePlacement` stay in TypeScript. `compose_radar_map`, `HttpClient`, and `symbolic_icon` stay in Rust.

Both contract tests load `fixtures/contract.json`, `zippopotam-55115.json`, `open-meteo-55115.json`, and the NWS alert fixtures. The Rust test is `crates/coldfront-core/tests/contract.rs`. The web test is `apps/coldfront-web/lib/contract.test.ts`.

## Upstream forecast

Both clients request the same Open-Meteo document. Units stay metric inside the model. Display code converts them.

The query uses these fields:

- `current`: `temperature_2m`, `relative_humidity_2m`, `apparent_temperature`, `weather_code`, `wind_speed_10m`
- `hourly`: `temperature_2m`, `weather_code`, `precipitation_probability`
- `daily`: `weather_code`, `temperature_2m_max`, `temperature_2m_min`, `precipitation_probability_max`
- `temperature_unit=celsius`
- `wind_speed_unit=kmh`
- `timezone=auto`
- `forecast_days=10`

A parsed forecast has one `Current`, hourly rows, and daily rows. The contract sample for ZIP 55115 has 240 hourly rows and 10 daily rows. Time strings are Open-Meteo local timestamps such as `2026-11-17T16:45`.

`next_hours` starts at the last hourly row whose time is less than or equal to `current.time`, then takes the requested count. The web page asks for 24. The first label is `Now` because the page passes `isNow` for index 0. Later labels are `5p` or `12a`. `day_label` returns `Today` when the date equals the current date, otherwise a short weekday.

Zippopotam `/us/{zip}` supplies `post code`, and the first `places` entry supplies `place name`, `state`, `state abbreviation`, `latitude`, and `longitude`.

NWS `/alerts/active?point={latitude},{longitude}` is a FeatureCollection. Each feature with a non-empty `properties.event` becomes an `Alert`. Empty events are dropped. The page and the popup render at most three alerts, event plus headline.

## Units and weather codes

Stored temperatures are Celsius. Stored wind is km/h. °F display converts with `celsius * 9 / 5 + 32` and then rounds. °C display rounds Celsius. Wind in °F mode is mph (`km/h / 1.609344`, then round). Wind in °C mode is rounded km/h.

Rounding is half away from zero. `-1.5` becomes `-2`. Non-finite values become `0`.

`describe` maps WMO codes to a label and an icon id:

| Codes | Label | Icon id |
| --- | --- | --- |
| 0 | Clear | `clear` |
| 1 | Mainly clear | `mainly-clear` |
| 2 | Partly cloudy | `partly-cloudy` |
| 3 | Overcast | `overcast` |
| 45, 48 | Fog | `fog` |
| 51, 53, 55 | Drizzle | `drizzle` |
| 56, 57 | Freezing drizzle | `freezing-drizzle` |
| 61, 63, 65 | Rain | `rain` |
| 66, 67 | Freezing rain | `freezing-rain` |
| 71, 73, 75, 77 | Snow | `snow` |
| 80 through 82 | Rain showers | `rain-showers` |
| 85, 86 | Snow showers | `snow-showers` |
| 95, 96, 99 | Thunderstorm | `thunder` |
| anything else | Unknown | `unknown` |

The web app draws those icon ids as SVG. The applet maps them through `symbolic_icon` to freedesktop names such as `weather-clear-symbolic`. A missing theme icon falls back inside COSMIC. The text label still shows. With no forecast loaded, the panel icon is `weather-clear-symbolic` and the temperature is hidden.

## Web request path

Routes are `/` and `/:zip`, for example `/55115`. There is no router library. `history.pushState` changes the path. A trailing slash is removed. The landing form rejects anything that is not a 5-digit ZIP.

Units live in `localStorage` under `coldfront-units`. Missing or unknown values mean °F. Changing units re-renders. It does not fetch.

`loadForecast` does this:

1. `GET /api/zip/{zip}` and `GET /api/config` together.
2. Parse the ZIP JSON with `parseZippopotam`. A 404 becomes "No place found for that ZIP code."
3. `GET /api/forecast?latitude=&longitude=` and `GET /api/alerts?latitude=&longitude=` together.
4. A failed forecast response rejects the page. A non-OK alerts response becomes an empty list. Alerts JSON that fails `parseAlerts` still rejects the page.
5. `basemap` is `maptiler` only when `/api/config` says `maptiler`. Otherwise it is `none`.

The conditions column then uses `describe`, `nextHours`, `hourLabel`, `dayLabel`, `displayTemperature`, and `displayWindMph`. `RadarMap` receives latitude, longitude, and the basemap flag.

The browser never sees the MapTiler key. The Worker adds the User-Agent that National Weather Service requires. A page script cannot set that header on a cross-origin `fetch`.

## Worker API

`worker/index.ts` handles `/api/*`. Any other path returns 404 when the Worker runs. Static files are served by the assets binding because only `/api/*` is in `run_worker_first`. Unknown asset paths use single-page application fallback.

Only `GET` and `HEAD` are allowed. Other methods return 405.

`planApi` chooses the upstream. `proxyApi` performs it. Response headers are `content-type` and `cache-control` only. The upstream URL is not copied into the response. Thrown failures return 502 and a log line with the path and error message, not the upstream URL.

`MAPTILER_API_KEY` is optional. It is not declared in `wrangler.jsonc`, so generated `Env` does not list it. The Worker reads it with a cast. An empty value means no basemap.

| Path | Result | Cache-Control |
| --- | --- | --- |
| `/api/config` | JSON `{ "basemap": "maptiler" }` or `{ "basemap": "none" }` | `no-store` |
| `/api/zip/{five digits}` | Zippopotam `/us/{zip}` body and status | `public, max-age=86400` |
| `/api/zip` and any other `/api/zip/...` | 400, `enter a 5-digit US ZIP code` | `no-store` |
| `/api/forecast?latitude&longitude` | Open-Meteo forecast | `public, max-age=300` |
| `/api/alerts?latitude&longitude` | `api.weather.gov` active alerts | `public, max-age=60` |
| `/api/map/{z}/{x}/{y}.png` | MapTiler `basic-v2` PNG | `public, max-age=86400` |
| Anything else under `/api/` | 404, `not found` | `no-store` |

Latitude must be finite and from -90 to 90. Longitude must be finite and from -180 to 180. Missing or out-of-range coordinates return 400.

Map tiles reject zoom above 18, or x or y outside `2 ** zoom`, with 400. With no key, `/api/map/...` returns 404 `basemap unavailable`.

Upstream requests set `user-agent` to `Coldfront/0.1 (https://github.com/drmaas/coldfront)` and `accept` to `application/json` or `image/png`.

Logs use `head_sampling_rate` 1, so every request can be logged. Traces use `head_sampling_rate` 0.01. `compatibility_date` is `2026-11-17`.

## Web radar

`RadarMap` draws on a 2D canvas. It does not use MapLibre or WebGL. Zoom starts at 6 and stays from 5 to 11. Drag pans. The loop is twelve frames, every five minutes, from 55 minutes ago through the latest scan. The loop interval is 750 ms. Playback starts on the latest frame.

The canvas fills `#d5e3ec`, draws MapTiler tiles when `basemap` is `maptiler`, then draws Iowa Environmental Mesonet (IEM) tiles, then draws a pin. IEM URLs go to `mesonet.agron.iastate.edu` from the browser. Basemap URLs go to `/api/map/{z}/{x}/{y}.png`. Image loads use `crossOrigin = "anonymous"` and a module-level promise cache.

IEM layer names:

- Latest scan: `nexrad-n0q-900913`
- Older frame: `nexrad-n0q-900913-m{minutes}m` with minutes zero-padded to two digits, for example `nexrad-n0q-900913-m55m`

Until the first frame draws radar pixels, the card says "Loading radar…". If that frame has no radar tile, it says "Radar tiles unavailable".

## Applet request path

`main` loads Fluent translations and runs `AppModel` as a COSMIC applet. App id is `io.github.drmaas.CosmicColdfront`.

`init` reads cosmic-config and does not fetch weather. The panel shows the default icon until a load succeeds. After that, the panel shows `symbolic_icon` for the current code plus the rounded temperature. Closing the popup keeps that memory. Opening it again does not fetch when both `forecast` and `frames` are already set.

The popup column order is fixed: summary, radar, current facts, alerts, settings. Settings are ZIP, °F / °C, MapTiler key, and the OSM toggle. Copy comes from `i18n/en/cosmic_coldfront.ftl`.

`start_load` bumps `generation`, clears frames, and returns immediately when the ZIP is invalid. A valid load does this:

1. `HttpClient::lookup_zip`, then `forecast`, then `alerts`.
2. A failed alert call becomes an empty list. ZIP and forecast errors fail the load.
3. `tile_window` at zoom 7 and a 3 by 3 tile span.
4. Fetch the basemap for those nine tiles.
5. Fetch and composite the latest radar frame (offset 0) first.
6. On `WeatherReady`, show that frame, then load older frames one at a time.

Results with a stale `generation` are ignored. A failed older frame is skipped. The next offset still loads.

`HttpClient` uses rustls, a 12 second timeout, and the Coldfront User-Agent. `get_bytes` maps HTTP 404 to `NotFound` and other non-success statuses to `Upstream`. Error text does not include the URL.

The radar loop runs only while the popup is open, playback is on, and at least two frames are stored. The interval is 800 ms. Playback defaults to on.

Config version is 1. Fields are `zip`, `fahrenheit` (default true), `maptiler_api_key`, and `allow_osm_basemap` (default false). Changing the shape requires a new `#[version]`. Unit changes save and do not reload. ZIP save, key save, and the OSM toggle reload when the popup is open. A cosmic-config update that changes the ZIP also reloads when the popup is open.

## Applet radar

`compose_radar_map` paints a pale background (`213, 227, 236`), blits basemap tiles, then blits radar tiles. Missing tiles leave that background or stay transparent. It crops a 512 by 340 region around the pin, resizes to 400 by 260 with a triangle filter, and draws the pin. The popup image widget shows that raster at a height of 240 px.

Frame offsets are 50, 40, 30, 20, 10, and 0 minutes. The latest frame is requested before the older ones so the popup can show a picture before the loop is full. If a composite exists but every radar tile was missing, the popup says the radar tiles are missing.

## Basemap

Web and applet choose a basemap differently.

The web app has two states. `/api/config` returns `maptiler` when the Worker secret is non-empty. Otherwise the canvas is radar and a pin on the plain background. The web app never calls OpenStreetMap.

The applet checks in this order:

1. Non-empty `MAPTILER_API_KEY` in the environment.
2. Non-empty key saved in cosmic-config.
3. OpenStreetMap raster tiles, only when "OSM basemap" is on.
4. Otherwise radar and the pin on a plain background.

> [!WARNING]
> OpenStreetMap public tile servers are not a fallback for a missing MapTiler key. The [OpenStreetMap tile usage policy](https://operations.osmfoundation.org/policies/tiles/) asks apps not to use those tiles heavily or from distributed installs. The toggle defaults to off. Turn it on only for a personal machine. Coldfront still sends its User-Agent when the toggle is on.

MapTiler tile path, when a key exists, is `https://api.maptiler.com/maps/basic-v2/{z}/{x}/{y}.png?key=...`. The web Worker builds that URL. The applet builds it in `maptiler_tile_url`. OSM tiles, applet only, are `https://tile.openstreetmap.org/{z}/{x}/{y}.png`.

## Failures

| Case | Web | Applet |
| --- | --- | --- |
| ZIP is not five digits | Inline error, no request | `Error::InvalidZip`, no request |
| Zippopotam 404 | "No place found for that ZIP code." | "no place found for that ZIP code" |
| Forecast HTTP or parse failure | Page error, no conditions | `map_error`, no new forecast |
| Alerts HTTP failure | Empty alert list, forecast still shows | Empty alert list, forecast still shows |
| Alerts body fails to parse | Page error | Empty alert list |
| MapTiler key missing | Radar on a plain background | Next choice in the [Basemap](#basemap) order |
| Radar tiles missing | "Radar tiles unavailable" | Caption that radar is missing |
| Stale in-flight load | AbortController on ZIP change | Drop messages with an old `generation` |

## What stays separate

- No database, accounts, or shared session.
- Web state is the URL, `localStorage` units, and memory. Applet settings are cosmic-config. Forecasts are memory only in both.
- The Worker is the only web credential boundary. The applet holds an optional key in the environment or in cosmic-config.
- CI runs tests and builds. It does not deploy the Worker. The Arch package builds only on a `v*` tag.
- Release version equality is the git tag, `apps/cosmic-coldfront` `version`, and `arch/PKGBUILD` `pkgver`. Core and web package versions are independent.
- Crates are `publish = false`. The license is GPL-3.0-only, matching libcosmic.

# Reference

Tables and field lists that support [architecture.md](architecture.md). Prefer the architecture file for design intent and diagrams.

## Contract lock

Change a shared rule in Rust, in TypeScript, and in `fixtures/contract.json` in one change.

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
| Day/night display icon id | `display_icon` / `displayIcon` | yes |
| Web Mercator tile math | `lat_lon_to_tile`, `tile_window` | yes |
| IEM tile URL and radar frame lists | `iem_tile_url`, `web_radar_frames`, `applet_radar_frames`, lookback lists, interval helpers, `format_radar_clock` | yes |
| Open-Meteo forecast query | `forecast_url` and `OPEN_METEO` | yes |
| Other HTTP to upstream hosts | `HttpClient` or the Worker | no |
| Painted map | `compose_radar_map`, `compose_viewport`, or `RadarMap.tsx` | no |
| Relief echo bands and ortho fit | `band_height` / `bandHeight`, `fit_ortho_half_extents` / `fitOrthoHalfExtents` | yes |
| Hourly chart and daily range bar | `chart_points` / `chartPoints`, `range_placement` / `rangePlacement` | yes |
| Panel icon names | `symbolic_icon` | no |

Both contract tests load `fixtures/contract.json`, `zippopotam-55115.json`, `open-meteo-55115.json`, and the NWS alert fixtures. Rust: `crates/coldfront-core/tests/contract.rs`. Web: `apps/coldfront-web/lib/contract.test.ts`.

## Upstream data

### Open-Meteo

Both clients request the same document. Units stay metric inside the model. Display code converts them.

Query fields:

- `current`: `temperature_2m`, `relative_humidity_2m`, `apparent_temperature`, `weather_code`, `wind_speed_10m`, `is_day`
- `hourly`: `temperature_2m`, `weather_code`, `precipitation_probability`, `is_day`
- `daily`: `weather_code`, `temperature_2m_max`, `temperature_2m_min`, `precipitation_probability_max`, `precipitation_sum`, `wind_speed_10m_max`, `relative_humidity_2m_mean`, `sunrise`, `sunset`
- `temperature_unit=celsius`
- `wind_speed_unit=kmh`
- `timezone=auto`
- `forecast_days=10`

`fixtures/contract.json` `open_meteo` holds that request as a base URL plus a canonical parameter map. Both contract tests compare their own client's forecast URL against it, so adding a field on one side only fails `pnpm test` or `cargo test -p coldfront-core`.

A parsed forecast has one `Current`, hourly rows, and daily rows. The contract sample for ZIP 55115 has 240 hourly rows and 10 daily rows. Time strings are Open-Meteo local timestamps such as `2026-11-17T16:45`.

`next_hours` starts at the last hourly row whose time is less than or equal to `current.time`, then takes the requested count. The web page asks for 24. The first label is `Now` because the page passes `isNow` for index 0. Later labels are `5p` or `12a`. `day_label` returns `Today` when the date equals the current date, otherwise a short weekday.

### Zippopotam and NWS

Zippopotam `/us/{zip}` supplies `post code`, and the first `places` entry supplies `place name`, `state`, `state abbreviation`, `latitude`, and `longitude`.

NWS `/alerts/active?point={latitude},{longitude}` is a FeatureCollection. Each feature with a non-empty `properties.event` becomes an `Alert`. Empty events are dropped. Page and popup render at most three alerts, event plus headline. An empty list shows "No active alerts" on the web page.

### Units and weather codes

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

`display_icon` / `displayIcon` takes a `describe` icon id and Open-Meteo `is_day`. At night, `clear`, `mainly-clear`, and `partly-cloudy` become `clear-night`, `mainly-clear-night`, and `partly-cloudy-night`. Other conditions keep the day icon. Current and hourly rows carry `is_day` from the place timezone. The 10-day list stays on day icons.

Web and applet popup SVGs draw rain streaks for `drizzle`, `rain`, and `rain-showers`; a mixed sleet mark (one rain streak and one snow mark) for `freezing-drizzle` and `freezing-rain`; and snow marks for `snow` and `snow-showers`. The panel maps those ids through `symbolic_icon`: rain and sleet ids use `weather-showers-symbolic`, snow ids use `weather-snow-symbolic`, and clear or cloudy ids use names such as `weather-clear-symbolic` or `weather-clear-night-symbolic`. A missing theme icon falls back inside COSMIC. The text label still shows. After startup, a valid saved ZIP loads the forecast so the panel shows the condition icon and the rounded temperature with its unit, for example `53°F`. On a horizontal panel the temperature sits to the right of the icon. On a vertical panel it sits under the icon. With no forecast loaded, the panel icon is `weather-clear-symbolic` and the temperature is hidden. Active alerts add `dialog-warning-symbolic` beside the weather icon.

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
| `/api/geocode/search?name=` | Open-Meteo geocoding (`countryCode=US`) | `public, max-age=86400` |
| `/api/forecast?latitude&longitude` | Open-Meteo forecast | `public, max-age=300` |
| `/api/alerts?latitude&longitude` | `api.weather.gov` active alerts | `public, max-age=60` |
| `/api/map/{z}/{x}/{y}.png` | MapTiler `basic-v2` PNG | `public, max-age=86400` |
| Anything else under `/api/` | 404, `not found` | `no-store` |

Latitude must be finite and from -90 to 90. Longitude must be finite and from -180 to 180. Missing or out-of-range coordinates return 400.

Map tiles reject zoom above 18, or x or y outside `2 ** zoom`, with 400. With no key, `/api/map/...` returns 404 `basemap unavailable`. Cross-origin or direct off-site use without a same-site `Origin` or `Referer` returns 403 and never reaches MapTiler. Per-IP rate limits return 429 when exceeded (Wrangler rate-limit binding at the edge; counts are approximate per PoP).

Upstream requests set `user-agent` to `Coldfront/0.1 (https://github.com/drmaas/coldfront)` and `accept` to `application/json` or `image/png`.

Logs use `head_sampling_rate` 1. Traces use `head_sampling_rate` 0.01. `compatibility_date` is `2026-11-17`.

### Web loadForecast steps

1. For a ZIP route, `GET /api/zip/{zip}` and `GET /api/config` together. Parse with `parseZippopotam`. A 404 becomes "No place found for that ZIP code."
2. For a coordinate route, restore the cached `Place` from `coldfront-place` when the coordinates match, or build a coordinate-only place for the forecast request.
3. `GET /api/forecast?latitude=&longitude=` and `GET /api/alerts?latitude=&longitude=` together.
4. A failed forecast response rejects the page. A non-OK alerts response becomes an empty list. Alerts JSON that fails `parseAlerts` still rejects the page.
5. `basemap` is `maptiler` only when `/api/config` says `maptiler`. Otherwise it is `none`.

The conditions column then uses `describe`, `nextHours`, `hourLabel`, `dayLabel`, `displayTemperature`, and `displayWindMph`. `RadarMap` receives latitude, longitude, and the basemap flag.

## Radar

### Web

`RadarMap` draws on a 2D canvas by default. It does not use MapLibre. Zoom starts at 6 and stays from 5 to 11 (fractional while pinching; tiles use `floor(zoom)` scaled up). Drag pans. Phones and touchscreens pinch-zoom toward the gesture center through Touch Events (`touch-action: none`, non-passive `preventDefault` so the page does not zoom instead). Pointer Events cover mouse drag and a no-capture multi-touch fallback. Trackpad two-finger scroll pans; pinch or mouse wheel zooms continuously toward the pointer. A recenter control clears pan so the ZIP pin returns to the middle. Default lookback is one hour: twelve frames, every five minutes, from 55 minutes ago through the latest scan. The timeline row has a lookback control (1h / 2h / 3h) on the left and a speed control (1× / 2× / 4× of 750 ms → 750, 375, 188) next to Play. Two- and three-hour lookbacks use five-minute steps starting at 120 and 180. Growing lookback fetches every new frame’s tiles at the current view before the loop steps onto them; the playhead waits on a still-pending frame and does not jump ahead. A top-left clock shows the even-minute scan time for the frame on screen (`h:mm AM/PM` in the forecast timezone). Defaults return on reload. Playback starts on the latest frame unless the OS asks for reduced motion (`prefers-reduced-motion: reduce`), which starts it paused; turning that setting on while the page is open also pauses. Play still works in both cases. The same controls and clock stay visible in fullscreen.

During pan and zoom, the stage scales and translates the last settled composite so the picture moves with the pointer. Settle work paints offscreen and swaps only when the camera is still and tiles are ready; it does not wipe the live canvas with pale holes. After 50 ms without a gesture (and on frame, place, size, or mode changes), it fetches any missing tiles first, then presents one composite. A settle does not show "Loading radar…". Image loads use `crossOrigin = "anonymous"` and a module-level promise cache. Each IEM radar tile is denoised once at 256×256 and kept in an LRU; later paints blit that canvas. After a settle, the map prefetches neighbor integer zooms and the next loop frame.

Canvas fills `#d5e3ec`, draws MapTiler tiles when `basemap` is `maptiler`, then draws cached IEM tiles, then draws a pin. IEM URLs go to `mesonet.agron.iastate.edu` from the browser. Basemap URLs go to `/api/map/{z}/{x}/{y}.png`.

When WebGL2 is available, a Flat / Relief control switches to `RadarRelief`. Relief keeps the same tiles, zoom range, frame list, and settle/gesture path. The same board transform moves the heightfield during pan and zoom; a settle rebuilds ground and echoes from the tile mosaic. Radar pixels are denoised per tile before Flat or Relief draws them. The ground is only the basemap (or the plain fill) plus the ZIP pin — not a copy of the Flat radar. Until you orbit, Relief keeps that same frame on the canvas and extrudes the wet cells up from it, in the widget and in fullscreen. `sampleEchoes` plus `buildEchoField` raise wet cells into an opaque heightfield on that same grid, with no extra spread; height follows the n0q color order (greens short, yellow mid, reds tall, purple tallest). Mesh lighting stays on the heightfield only; the ground texture is drawn unshaded. Drag, wheel, zoom buttons, recenter, and fullscreen use the same map pan/zoom as Flat. Shift-drag or right-drag orbits; recenter returns to the pixel-locked view. An empty echo field keeps the ground and shows "No echoes in this view." Without WebGL2 the control stays hidden and the view stays flat.

Until the first frame draws radar pixels, the card says "Loading radar…". If a settle finishes with no radar tile, it says "Radar tiles unavailable".

### Applet

The Radar tab uses `map_viewport` and `compose_viewport`. The camera starts at zoom 7 with zero pan. Zoom stays from 5 to 11 (fractional while scrolling; tiles use `floor(zoom)` scaled up). Drag pans. Mouse wheel zooms toward the pointer. Trackpad pixel scroll pans; Ctrl plus scroll zooms. Ctrl plus scroll uses the same coalesced gesture paint as pan, so a touchpad zoom burst still paints at most once per 16 ms frame and `snapshot_transform` scales the last frame toward the new zoom between paints. A hint chip under the clock reads “Drag to pan · Ctrl + scroll to zoom” (plus “Right-drag to orbit” in Relief) until the first map gesture. Touchpad pinch reaches no applet: cosmic-comp forwards pinch only through `zwp_pointer_gestures_v1`, which iced (as pinned) does not bind, so Ctrl plus two-finger scroll is the applet's pinch equivalent. Zoom in/out, recenter, expand/restore, and a 2D / 3D mode toggle overlay the map (same idea as the web). An on-map timeline row holds lookback, the scrubber, Now, Play, and speed in both compact and expanded views so the 480×440 popup chrome cannot clip them. Compact view size is 416 by 220; expanded view size follows the monitor-sized popup minus header and tabs. Pan and zoom use a `mouse_area` on the canvas only; empty overlay regions report no hit so gestures reach the map. Tiles are cached by source, zoom, and coordinates. During pan and zoom, the stage recomposes from the tile cache at the live camera at most once per 16 ms frame, at one device pixel per logical pixel, and inside a 0.5 MP budget (larger views render smaller and the canvas scales them up); `snapshot_transform` slides the last frame between paints so the map keeps up with the pointer, and settle repaints at the display pixel ratio. `snapshot_transform` alone only covers a failed cache paint. After 50 ms without a gesture (and on frame, place, size, or mode changes), settle fetches missing tiles first, then presents one composite when the camera is still. A settle does not show "Loading radar…" over an existing snapshot. Each IEM radar tile is denoised once at 256×256 and kept in an LRU of 512. After a settle, the map prefetches neighbor integer zooms and the next loop frame. Failed tile fetches stay absent so the next settle can retry. Stale fetch tasks still merge successful tiles into the cache.

Relief mode (session only; default is Flat) paints through `compose_viewport_ground`, `compose_viewport_radar_layer`, and `render_relief` in `coldfront-core`. The ground is basemap or plain fill plus the ZIP pin. Radar tiles are already denoised in the cache, then sampled into the same n0q heightfield rules as the web and extruded on that board. Map pan and zoom use the same snapshot transform as Flat; settle rebuilds ground and echoes. Shift-drag or right-drag orbits; left-drag still pans the map. Recenter clears pan and returns to the locked camera. An empty echo field keeps the ground and shows "No echoes in this view." The frame caption adds " · Relief" while 3D is on.

`compose_radar_map` still paints a pale background (`213, 227, 236`), blits basemap and radar tiles, crops around the pin, and resizes to 400 by 260. That path stays for unit and contract tests. The interactive tab does not use it.

Default lookback is one hour: offsets 50, 40, 30, 20, 10, and 0 minutes. Lookback (1h / 2h / 3h) and speed (1× / 2× / 4× of 800 ms → 800, 400, 200) sit on the timeline row. Two- and three-hour lookbacks use ten-minute steps starting at 120 and 180. Closing the popup keeps the choice; an applet restart returns to 1× and 1h. Growing lookback fetches missing offsets in one batch at the current camera; shrinking drops offsets outside the new list. The loop advances only onto frames that already have radar tiles, waits while the next offset is still pending, and skips offsets that returned zero tiles. A top-left clock shows the even-minute scan time for the shown frame in the forecast timezone. The latest frame is requested before the older ones so the tab can show a picture before the loop is full. If a composite exists but every radar tile was missing, the tab says the radar tiles are missing.

The radar loop runs only while the popup is open on the Radar tab, playback is on, and at least two frames with radar tiles are stored. Playback defaults to on.

`HttpClient` uses rustls, a 12 second timeout, and the Coldfront User-Agent. `get_bytes` maps HTTP 404 to `NotFound` and other non-success statuses to `Upstream`. Error text does not include the URL.

### IEM layer names

Radar uses MRMS hybrid-scan reflectivity (cleaner than NEXRAD base reflectivity against ground clutter).

- Latest scan: `q2-hsr-900913`
- Older frame: `mrms::lcref-{YYYYMMDDHHMI}` where the stamp is UTC `now - offset`, floored to an even minute (MRMS archive step is 2 minutes). Example at `2026-11-20T05:10:00Z` with offset 55: `mrms::lcref-202611200414`

### Basemap URLs

MapTiler tile path, when a key exists: `https://api.maptiler.com/maps/basic-v2/{z}/{x}/{y}.png?key=...`. The web Worker builds that URL. The applet builds it in `maptiler_tile_url`. OSM tiles, applet only: `https://tile.openstreetmap.org/{z}/{x}/{y}.png`.

Applet basemap order: environment `MAPTILER_API_KEY`, then cosmic-config key, then OSM if `allow_osm_basemap` is on, otherwise none. Web never calls OpenStreetMap. See [architecture basemap](architecture.md#radar-and-basemap) for the OSM warning.

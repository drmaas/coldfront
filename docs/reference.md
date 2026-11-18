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
| Web Mercator tile math | `lat_lon_to_tile`, `tile_window` | yes |
| IEM tile URL and radar frame lists | `iem_tile_url`, `web_radar_frames`, `applet_radar_frames` | yes |
| HTTP to upstream hosts | `HttpClient` or the Worker | no |
| Painted map | `compose_radar_map` or `RadarMap.tsx` | no |
| Hourly chart and daily range bar | `chartPoints`, `rangePlacement` | no |
| Panel icon names | `symbolic_icon` | no |

Both contract tests load `fixtures/contract.json`, `zippopotam-55115.json`, `open-meteo-55115.json`, and the NWS alert fixtures. Rust: `crates/coldfront-core/tests/contract.rs`. Web: `apps/coldfront-web/lib/contract.test.ts`.

## Upstream data

### Open-Meteo

Both clients request the same document. Units stay metric inside the model. Display code converts them.

Query fields:

- `current`: `temperature_2m`, `relative_humidity_2m`, `apparent_temperature`, `weather_code`, `wind_speed_10m`
- `hourly`: `temperature_2m`, `weather_code`, `precipitation_probability`
- `daily`: `weather_code`, `temperature_2m_max`, `temperature_2m_min`, `precipitation_probability_max`
- `temperature_unit=celsius`
- `wind_speed_unit=kmh`
- `timezone=auto`
- `forecast_days=10`

A parsed forecast has one `Current`, hourly rows, and daily rows. The contract sample for ZIP 55115 has 240 hourly rows and 10 daily rows. Time strings are Open-Meteo local timestamps such as `2026-11-17T16:45`.

`next_hours` starts at the last hourly row whose time is less than or equal to `current.time`, then takes the requested count. The web page asks for 24. The first label is `Now` because the page passes `isNow` for index 0. Later labels are `5p` or `12a`. `day_label` returns `Today` when the date equals the current date, otherwise a short weekday.

### Zippopotam and NWS

Zippopotam `/us/{zip}` supplies `post code`, and the first `places` entry supplies `place name`, `state`, `state abbreviation`, `latitude`, and `longitude`.

NWS `/alerts/active?point={latitude},{longitude}` is a FeatureCollection. Each feature with a non-empty `properties.event` becomes an `Alert`. Empty events are dropped. Page and popup render at most three alerts, event plus headline.

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

Web draws those icon ids as SVG. Applet maps them through `symbolic_icon` to freedesktop names such as `weather-clear-symbolic`. A missing theme icon falls back inside COSMIC. The text label still shows. After startup, a valid saved ZIP loads the forecast so the panel icon matches the current condition. With no forecast loaded, the panel icon is `weather-clear-symbolic` and the temperature is hidden. Active alerts add `dialog-warning-symbolic` beside the weather icon.

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

Logs use `head_sampling_rate` 1. Traces use `head_sampling_rate` 0.01. `compatibility_date` is `2026-11-17`.

### Web loadForecast steps

1. `GET /api/zip/{zip}` and `GET /api/config` together.
2. Parse the ZIP JSON with `parseZippopotam`. A 404 becomes "No place found for that ZIP code."
3. `GET /api/forecast?latitude=&longitude=` and `GET /api/alerts?latitude=&longitude=` together.
4. A failed forecast response rejects the page. A non-OK alerts response becomes an empty list. Alerts JSON that fails `parseAlerts` still rejects the page.
5. `basemap` is `maptiler` only when `/api/config` says `maptiler`. Otherwise it is `none`.

The conditions column then uses `describe`, `nextHours`, `hourLabel`, `dayLabel`, `displayTemperature`, and `displayWindMph`. `RadarMap` receives latitude, longitude, and the basemap flag.

## Radar

### Web

`RadarMap` draws on a 2D canvas. It does not use MapLibre or WebGL. Zoom starts at 6 and stays from 5 to 11 (fractional while pinching; tiles use `floor(zoom)` scaled up). Drag pans. Trackpad two-finger scroll pans; pinch or mouse wheel zooms continuously toward the pointer. A recenter control clears pan so the ZIP pin returns to the middle. The loop is twelve frames, every five minutes, from 55 minutes ago through the latest scan. The loop interval is 750 ms. Playback starts on the latest frame.

Canvas fills `#d5e3ec`, draws MapTiler tiles when `basemap` is `maptiler`, then draws IEM tiles, then draws a pin. IEM URLs go to `mesonet.agron.iastate.edu` from the browser. Basemap URLs go to `/api/map/{z}/{x}/{y}.png`. Image loads use `crossOrigin = "anonymous"` and a module-level promise cache.

Until the first frame draws radar pixels, the card says "Loading radar…". If that frame has no radar tile, it says "Radar tiles unavailable".

### Applet

The Radar tab uses `map_viewport` and `compose_viewport`. The camera starts at zoom 7 with zero pan. Zoom stays from 5 to 11. Drag pans. Mouse wheel zooms. Trackpad pixel scroll pans; Ctrl plus scroll zooms. Zoom in/out and recenter buttons sit under the map. View size is 880 by 520. Tiles are cached by source, zoom, and coordinates; pan recomposites from cache, and a short settle delay fetches newly visible edge tiles.

`compose_radar_map` still paints a pale background (`213, 227, 236`), blits basemap and radar tiles, crops around the pin, and resizes to 400 by 260. That path stays for unit and contract tests. The interactive tab does not use it.

Frame offsets are 50, 40, 30, 20, 10, and 0 minutes. The latest frame is requested before the older ones so the tab can show a picture before the loop is full. If a composite exists but every radar tile was missing, the tab says the radar tiles are missing.

The radar loop runs only while the popup is open on the Radar tab, playback is on, and at least two frames are stored. Interval is 800 ms. Playback defaults to on.

`HttpClient` uses rustls, a 12 second timeout, and the Coldfront User-Agent. `get_bytes` maps HTTP 404 to `NotFound` and other non-success statuses to `Upstream`. Error text does not include the URL.

### IEM layer names

- Latest scan: `nexrad-n0q-900913`
- Older frame: `nexrad-n0q-900913-m{minutes}m` with minutes zero-padded to two digits, for example `nexrad-n0q-900913-m55m`

### Basemap URLs

MapTiler tile path, when a key exists: `https://api.maptiler.com/maps/basic-v2/{z}/{x}/{y}.png?key=...`. The web Worker builds that URL. The applet builds it in `maptiler_tile_url`. OSM tiles, applet only: `https://tile.openstreetmap.org/{z}/{x}/{y}.png`.

Applet basemap order: environment `MAPTILER_API_KEY`, then cosmic-config key, then OSM if `allow_osm_basemap` is on, otherwise none. Web never calls OpenStreetMap. See [architecture basemap](architecture.md#radar-and-basemap) for the OSM warning.

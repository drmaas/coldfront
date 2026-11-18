# AGENTS.md

Rules for `crates/coldfront-core`. Also follow the repository [AGENTS.md](../../AGENTS.md) and [docs/architecture.md](../../docs/architecture.md).

## When to use

Use this file when you edit parsers, display math, the HTTP client, or radar compositing.

## When not to use

Do not put COSMIC widgets, Fluent strings, or Worker routes in this crate. The applet calls this crate. The web app copies the shared functions by hand.

## Rules

- Keep models in metric units: Celsius, km/h, and WMO codes. Convert in `units` at display time.
- `round_half_away_from_zero` is half away from zero. Non-finite input returns `0`.
- `is_us_zip` accepts only five ASCII digits.
- Open-Meteo query parameters in `HttpClient::forecast` must match `OPEN_METEO` in `apps/coldfront-web/lib/server/proxy.ts`.
- `web_radar_frames` is twelve frames, every five minutes, from 55 through 0. `applet_radar_frames` is offsets 50, 40, 30, 20, 10, 0. Layer names stay `nexrad-n0q-900913` and `nexrad-n0q-900913-m{mm}m`.
- `compose_radar_map` output stays 400 by 260. Do not call it from web code.
- `get_bytes` must not put the URL in `Error`. A MapTiler key can sit in that URL.
- `osm_tile_url` is for an explicit applet opt-in. Do not call it from a default path in this crate.
- A shared-rule change updates this crate, `lib/domain.ts`, and `fixtures/contract.json` together.
- Stay `publish = false`. Keep the SPDX header `GPL-3.0-only` on Rust files.
- Do not add a dependency the web contract would silently ignore. If both clients must share a new rule, the TypeScript copy is part of the change.

## Commands

From the repository root:

```bash
cargo test -p coldfront-core --locked
cargo clippy -p coldfront-core --all-targets --locked -- -D warnings
```

The contract test reads `fixtures/` relative to this crate. Do not change that path without moving the fixtures.

## Failures

- `Error::InvalidZip` for a bad ZIP, before any HTTP call.
- `Error::NotFound` for HTTP 404.
- `Error::Upstream(status)` for any other non-success status.
- `Error::Parse` when required JSON is missing or the wrong type. Alert features with an empty `event` are skipped, not parse errors.
- `compose_radar_map` returns `Error::Parse` when the grid width or height is 0.

## Example

Good: change `hour_label`, the TypeScript twin, and the `next_hour_labels` array in `fixtures/contract.json`.

Bad: change `hour_label` and leave `apps/coldfront-web/lib/domain.ts` alone.

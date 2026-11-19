# AGENTS.md

Rules for code changes in this repository. Read [docs/architecture.md](docs/architecture.md) before changing data flow, APIs, or shared display rules. Read [CONTRIBUTING.md](CONTRIBUTING.md) for setup and release steps. Do not invent behavior that those files do not describe.

## When to use

Use this file for any change under this repository.

Also read the nearest package file before editing that package:

- [apps/coldfront-web/AGENTS.md](apps/coldfront-web/AGENTS.md)
- [apps/cosmic-coldfront/AGENTS.md](apps/cosmic-coldfront/AGENTS.md)
- [crates/coldfront-core/AGENTS.md](crates/coldfront-core/AGENTS.md)
- [.github/workflows/AGENTS.md](.github/workflows/AGENTS.md)

## When not to use

Do not put end-user setup steps here. Put them in `CONTRIBUTING.md`.

Do not treat the web app and the applet as one program. They do not share a binary, a process, or a network API.

## Rules

- Keep shared display rules in `crates/coldfront-core` and `apps/coldfront-web/lib/domain.ts`. Do not put those rules only in a React component or an iced view.
- When you change ZIP checks, parsers, rounding, labels, tile math, IEM URLs, or radar frame lists, change Rust, TypeScript, and `fixtures/contract.json` in the same change. Both contract tests must pass.
- Leave web-only chart helpers (`chartPoints`, `rangePlacement`) in TypeScript. Leave applet compositing (`compose_radar_map`, `compose_viewport`) and `symbolic_icon` in Rust.
- Request Open-Meteo in Celsius and km/h with `forecast_days=10`. Convert only at display time.
- Round display values half away from zero. `-1.5` becomes `-2`. Non-finite input becomes `0`.
- A US ZIP code is five ASCII digits. Leading zeros count. Reject ZIP+4.
- Send `Coldfront/0.1 (https://github.com/drmaas/coldfront)` on upstream requests that Coldfront makes itself.
- Do not log a URL that can contain `MAPTILER_API_KEY`. `HttpClient` errors omit the URL. Keep that property.
- Do not commit `MAPTILER_API_KEY`. Do not put it in `wrangler.jsonc`, source, or a tracked env file.
- Do not fetch OpenStreetMap tiles unless the applet user turns that setting on. Do not use OSM as the web basemap.
- Show at most three alerts. A failed alert request must not hide a forecast that already loaded. See [Failures](docs/architecture.md#failures) for the current split.
- The applet popup tabs are Current, Radar, Hourly, and 10-day, with Settings and Alerts from the header. Radar uses the interactive viewport, not a stacked image above the facts.
- Do not create a `v*` tag to try the release workflow. Site deploy is the `deploy` job in `ci.yml` after checks on a push to `main`, not a `v*` tag.
- New Rust source keeps `SPDX-License-Identifier: GPL-3.0-only`. Crates stay `publish = false`.
- Do not lower `rust-version` below 1.93. libcosmic requires it. CI uses the stable toolchain in `rust-toolchain.toml`.

## Where to change what

| Change | Edit |
| --- | --- |
| Shared parse, units, tile math, frame lists | `crates/coldfront-core` and `apps/coldfront-web/lib/domain.ts`, then `fixtures/contract.json` |
| Worker routes, cache, upstream URLs | `apps/coldfront-web/lib/server/proxy.ts` and `worker/index.ts` |
| Web page behavior | `apps/coldfront-web/src` |
| Applet popup behavior | `apps/cosmic-coldfront/src/app.rs` |
| Applet popup tabs and layout | `apps/cosmic-coldfront/src/view.rs` |
| Applet tile fetch and composite | `apps/cosmic-coldfront/src/mapload.rs` calling `coldfront-core` |
| Saved ZIP, units, key, OSM toggle | `apps/cosmic-coldfront/src/config.rs` |
| Applet strings | `apps/cosmic-coldfront/i18n/en/cosmic_coldfront.ftl` |
| Arch package | `arch/PKGBUILD` and `apps/cosmic-coldfront/Cargo.toml` version together |
| CI, deploy, or the Arch release workflow | `.github/workflows` |

## Commands

From the repository root:

```bash
just test
just check
```

`just test` runs `cargo test --workspace --locked` and `pnpm test`. `just check` also runs fmt, clippy `-D warnings`, the applet build, Worker type check, web lint, typecheck, and web build.

## Failure behavior

- Invalid ZIP: do not call upstream. Say to enter a 5-digit US ZIP code.
- Unknown ZIP: Zippopotam 404 becomes "no place found for that ZIP code".
- Forecast failure: show the error. Do not render a partial forecast.
- Stale applet loads: ignore results whose `generation` is not current.

## Examples

Good: change `round_half_away_from_zero` and `roundHalfAwayFromZero`, update `fixtures/contract.json`, and run both contract tests.

Bad: change only the React temperature text and leave the applet on the old rounding.

Good: read `MAPTILER_API_KEY` from the Worker env or from cosmic-config.

Bad: write the key into `wrangler.jsonc` or into a log line.

# AGENTS.md

Rules for `apps/coldfront-web`. Also follow the repository [AGENTS.md](../../AGENTS.md) and [docs/architecture.md](../../docs/architecture.md).

## When to use

Use this file when you edit the site, the Worker, or `lib/`.

## When not to use

Do not add applet UI here. Do not call `coldfront-core` from this package. Keep the TypeScript copy in `lib/domain.ts`.

## Rules

- Put shared parse, unit, label, and tile rules in `lib/domain.ts`. Put route planning in `lib/server/proxy.ts`. Put page behavior in `src/`.
- Do not fetch Zippopotam, Open-Meteo, National Weather Service, or MapTiler from a component. Call `/api/*`.
- IEM radar tiles load in the browser from `iemTileUrl`. Do not proxy them through the Worker unless the architecture file changes too.
- `MAPTILER_API_KEY` stays a Worker secret. `/api/config` returns `basemap` only. Never return the key.
- Do not declare that secret in `wrangler.jsonc`. An empty key must still deploy.
- Do not log upstream URLs.
- Accept only `GET` and `HEAD` on `/api/*`.
- Keep cache lifetimes: ZIP and map tiles 86400 seconds, forecast 300, alerts 60, config and errors `no-store`.
- Changing a function that the contract test imports requires the same Rust change and a `fixtures/contract.json` update.
- `chartPoints` and `rangePlacement` stay mirrored with `chart_points` and `range_placement` in Rust. Change both and the contract fixture together.
- Unit changes read `localStorage` key `coldfront-units`. Do not refetch on °F / °C.
- Theme preference reads `localStorage` key `coldfront-theme` (`light` | `dark` | `system`). Resolve `system` with `prefers-color-scheme` and set `data-theme` on `document.documentElement` to `light` or `dark` only.
- Show at most three alerts.
- Radar stays a 2D canvas. Zoom stays from 5 to 11. The loop stays twelve frames at 750 ms.
- After a binding change, run `pnpm exec wrangler types --include-runtime false` and commit `worker-configuration.d.ts`.
- Keep `pnpm-workspace.yaml` `allowBuilds` for `esbuild` and `workerd`. pnpm 12 blocks those install scripts otherwise.

## Commands

From this directory:

```bash
pnpm test
pnpm run lint
pnpm run typecheck
pnpm run build
pnpm exec wrangler types --include-runtime false --check
```

From the repository root, `just check` includes those steps.

## Failures

- Non-`/api` requests that reach the Worker return 404. Assets still serve the single-page app.
- Bad ZIP paths return 400 plain text.
- Missing coordinates return 400.
- Tile zoom above 18, or x or y out of range, returns 400.
- Missing MapTiler key on `/api/map/...` returns 404 `basemap unavailable`.
- Upstream network failure returns 502 `upstream failed`.
- A non-OK alerts response becomes `[]` in `loadForecast`. A forecast failure rejects the page. Alerts JSON that throws in `parseAlerts` also rejects the page.

## Example

Good: add a forecast field in `lib/domain.ts`, mirror it in `coldfront-core`, and extend the contract fixture.

Bad: format a new temperature only inside `App.tsx`.

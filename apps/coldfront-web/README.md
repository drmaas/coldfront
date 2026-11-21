# coldfront-web

Vite and React site for Coldfront. A Cloudflare Worker sits in front of `/api/*`. Static assets serve the rest. This is Workers static assets, not Cloudflare Pages.

Enter a 5-digit US ZIP code or a US city name. Five digits still use Zippopotam. City search uses Open-Meteo geocoding through `/api/geocode/search`. The page shows current conditions, the next 24 hours, a 10-day forecast, a radar canvas, and up to three National Weather Service alerts. Routes are `/`, `/:zip`, and `/@{lat},{lon}` for saved coordinates.

The browser never holds the MapTiler key. The Worker also sets the User-Agent that the National Weather Service requires.

Shared display rules live in `lib/domain.ts`. They must match `crates/coldfront-core`. The design record is [docs/architecture.md](../../docs/architecture.md). Local setup and deploy are in [CONTRIBUTING.md](../../CONTRIBUTING.md).

## Layout

```text
lib/domain.ts           parsers, units, labels, tile math
lib/server/proxy.ts     /api route plan and upstream fetch
worker/index.ts         Worker entry
src/App.tsx             landing page and forecast page
src/RadarMap.tsx        canvas radar
```

## Run

From this directory:

```bash
cp .dev.vars.example .dev.vars
pnpm install
pnpm run dev
```

The dev server is `http://localhost:5173`. An empty `MAPTILER_API_KEY` means radar without streets. Do not commit `.dev.vars`.

```bash
pnpm test
pnpm run lint
pnpm run typecheck
pnpm run build
```

`pnpm test` runs Vitest on `lib/**/*.test.ts`. It does not start the Worker.

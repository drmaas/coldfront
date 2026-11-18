# coldfront-core

Rust domain crate for the COSMIC applet. It looks up a US ZIP code, parses Open-Meteo and National Weather Service JSON, rounds display values, and builds NEXRAD tile URLs. It can also composite a 3 by 3 radar image.

The web app does not link this crate. `apps/coldfront-web/lib/domain.ts` repeats the shared rules. `fixtures/contract.json` is the lock. The design record is [docs/architecture.md](../../docs/architecture.md).

The crate is `publish = false` and GPL-3.0-only.

## Modules

| Module | Role |
| --- | --- |
| `client` | `HttpClient` for Zippopotam, Open-Meteo, and alerts |
| `place`, `forecast`, `alerts` | JSON parsers and models |
| `units` | Celsius, Fahrenheit, mph, and half-away-from-zero rounding |
| `weather` | WMO code labels and freedesktop icon names |
| `radar` | Tile math, frame lists, and `compose_radar_map` |
| `zipcode` | Five-digit US ZIP check |

`HttpClient` sends `Coldfront/0.1 (https://github.com/drmaas/coldfront)` and times out after 12 seconds. Errors do not include the request URL.

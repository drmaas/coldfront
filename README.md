# coldfront

![Coldfront applet icon](apps/cosmic-coldfront/resources/icons/hicolor/scalable/apps/io.github.drmaas.CosmicColdfront.svg)

Coldfront shows US weather for a ZIP code. The license is GPL-3.0-only.

The web app shows current conditions, the next 24 hours, a 10-day forecast, live NEXRAD radar, and active National Weather Service alerts. The COSMIC applet shows current conditions, the radar image, and those alerts in a panel popup.

The two apps do not share a binary. They follow the same display rules. [Architecture](docs/architecture.md) is the design record. [Contributing](CONTRIBUTING.md) covers local setup, tests, deploy, and release.

## Repository

```text
apps/coldfront-web      Vite + React site on a Cloudflare Worker
apps/cosmic-coldfront   COSMIC panel applet (libcosmic / iced)
crates/coldfront-core   Rust weather client, parsers, and radar math
fixtures/               JSON samples and contract.json
arch/                   Arch Linux PKGBUILD for the applet
```

## Data sources

Forecasts are requested in Celsius and km/h. The app converts them for display. Switching °F and °C does not fetch again.

| Source | Use | Credential |
| --- | --- | --- |
| [Zippopotam.us](https://api.zippopotam.us) | ZIP code to city and coordinates | none |
| [Open-Meteo](https://open-meteo.com/) | Current, hourly, and daily forecast | none |
| [api.weather.gov](https://www.weather.gov/documentation/services-web-api) | Active alerts | none; a User-Agent is required |
| [Iowa Environmental Mesonet](https://mesonet.agron.iastate.edu/GIS/ridge.phtml) | NEXRAD base-reflectivity tiles | none |
| [MapTiler](https://www.maptiler.com/) | Optional street basemap | optional API key |

Every upstream request from Coldfront sends `Coldfront/0.1 (https://github.com/drmaas/coldfront)`.

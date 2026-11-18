# cosmic-coldfront

COSMIC panel applet for Coldfront. App id is `io.github.drmaas.CosmicColdfront`. It links `crates/coldfront-core` and does not talk to the web Worker.

The panel button shows a symbolic weather icon. After a successful load it also shows the temperature. The popup opens in this order: one-line summary, radar image, current facts, up to three alerts, then settings. The radar is not behind a tab.

Settings are the ZIP code, °F or °C, an optional MapTiler key, and an OpenStreetMap basemap toggle. That toggle defaults to off. Turn it on only on a personal machine. See [Architecture](../../docs/architecture.md#radar-and-basemap) for the basemap order.

The applet fetches a 10-day forecast and displays current conditions. It does not draw the hourly chart or the 10-day list.

Strings live in `i18n/en/cosmic_coldfront.ftl`. The desktop file, metainfo, and icon under `resources/` are installed as-is.

## Run

From the repository root, with COSMIC build libraries installed:

```bash
just applet build-release
just applet run
just applet install
```

`just applet install` writes `/usr/bin/cosmic-coldfront` plus the desktop file, metainfo, and icon. Full steps are in [CONTRIBUTING.md](../../CONTRIBUTING.md).

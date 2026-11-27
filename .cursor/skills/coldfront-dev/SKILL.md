---
name: coldfront-dev
description: >-
  Build, run, install, and uninstall the Coldfront COSMIC applet, restart the
  COSMIC panel, and run the web app locally. Use whenever the user wants to
  build, run, start, install, uninstall, reload, or restart cosmic-coldfront or
  the panel applet, or run, start, or preview the web app, even if they only
  mention just, cargo, pnpm, sudo, the panel, or localhost and do not name this
  skill. Includes the sudo PATH trap, the --no-deps install and uninstall
  commands, and the panel restart script.
---

# Coldfront local dev

Run commands from the repository root unless a step says otherwise. `just applet` forwards to `apps/cosmic-coldfront/justfile`.

## Applet

Build and run as the logged-in user. The applet needs that user's Wayland session. `cargo` lives in `~/.cargo/bin`. `sudo` resets `PATH`, so root cannot see `cargo`. A root build also leaves `target/` owned by root.

```bash
just applet build-release
just applet run
```

Do not prefix those with `sudo`.

Install and uninstall write four files under `/usr`: `/usr/bin/cosmic-coldfront`, the desktop file, the metainfo, and the icon. The `install` recipe depends on `build-release`. Root must not run that dependency. Build first as the user, then copy or delete without rebuilding:

```bash
sudo just --justfile apps/cosmic-coldfront/justfile --no-deps install
```

```bash
sudo just --justfile apps/cosmic-coldfront/justfile --no-deps uninstall
```

`--no-deps` skips `build-release`. `sudo just applet install` rebuilds as root, fails to find `cargo`, and can leave `target/` owned by root.

The panel keeps the process that was already running, so an installed binary does not take effect until the panel restarts it. Ask the user whether to restart the panel now. Do not restart it unasked: the panel and every applet on it briefly disappear.

```bash
scripts/restart-panel.sh
```

The script sends `cosmic-panel` SIGTERM, `cosmic-session` respawns it, and the applets, `cosmic-coldfront` included, come back as children of the new panel. `--dry-run` prints the plan and stops. `--timeout SECONDS` changes the wait for the respawn (default 15).

When the user declines, say the new binary loads on the next panel restart or login, and leave the running applet alone.

A saved ZIP, unit choice, MapTiler key, and the OSM toggle live in cosmic-config for app id `io.github.drmaas.CosmicColdfront`. The forecast itself is memory only. Open the popup again after restart to load weather.

## Web app

From `apps/coldfront-web`:

```bash
cp .dev.vars.example .dev.vars
pnpm install
pnpm run dev
```

Skip the `cp` when `.dev.vars` already exists. Open `http://localhost:5173`. Leave `MAPTILER_API_KEY` empty to show radar without streets. `.dev.vars` is gitignored. Do not commit it. Do not deploy, and do not run `wrangler deploy`, unless the user asks for a deploy.

## Leave alone

`just test`, `just check`, Worker deploy, and the Arch release are in `CONTRIBUTING.md`. Do not run them as part of a local applet run, install, or web preview.

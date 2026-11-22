# Contributing

This file is how to run Coldfront locally, how to test it, and how to deploy or release it.

Design rules live in [docs/architecture.md](docs/architecture.md). Field lists and Worker routes live in [docs/reference.md](docs/reference.md). Agent rules live in [AGENTS.md](AGENTS.md).

## Prerequisites

Install these tools before the first run:

- Rust stable, with rustfmt and clippy. `rust-toolchain.toml` selects that toolchain. Do not use a compiler older than 1.93. Current libcosmic rejects it.
- Node.js 26 (current LTS) and pnpm 12. `.nvmrc` and `packageManager` in `apps/coldfront-web` pin those. CI installs them with `pnpm/setup`.
- [`just`](https://github.com/casey/just). The root `justfile` is the entry point.

The applet also needs the system libraries from the `rust` job in `.github/workflows/ci.yml`. On Debian and Ubuntu that set includes Wayland, libxkbcommon, OpenSSL, fontconfig, FreeType, expat, Vulkan, libinput, gbm, udev, dbus, EGL, and GL.

## Web app

Run these commands from `apps/coldfront-web`:

```bash
cp .dev.vars.example .dev.vars
pnpm install
pnpm run dev
```

Open `http://localhost:5173`. Leave `MAPTILER_API_KEY` empty to show radar without streets. `.dev.vars` and `.env` are gitignored. Do not commit them.

Other scripts in that package:

```bash
pnpm test
pnpm run lint
pnpm run typecheck
pnpm run build
pnpm run preview
```

`pnpm test` runs Vitest on `lib/**/*.test.ts` only. It does not boot the Worker.

After a Worker binding change, regenerate the checked-in types and commit the diff:

```bash
pnpm exec wrangler types --include-runtime false
```

CI checks that file with `--check`. A stale `worker-configuration.d.ts` fails the web job.

## Applet

Run these commands from the repository root. `just applet` forwards to `apps/cosmic-coldfront/justfile`.

```bash
just applet build-release
just applet run
```

Run those as yourself. `just applet run` needs your Wayland session. `sudo` drops `~/.cargo/bin` from `PATH`, so root cannot run `cargo`.

Install and uninstall write under `/usr`. Build the release binary first. `install` depends on `build-release`, and `--no-deps` skips that rebuild so root only copies or deletes files:

```bash
just applet build-release
sudo just --justfile apps/cosmic-coldfront/justfile --no-deps install
```

```bash
sudo just --justfile apps/cosmic-coldfront/justfile --no-deps uninstall
```

`install` writes `/usr/bin/cosmic-coldfront`, the desktop file, the metainfo, and the icon. `uninstall` removes those four files. `sudo just applet install` rebuilds as root, fails to find `cargo`, and can leave `target/` owned by root.

The desktop file, metainfo, and icon are static files under `apps/cosmic-coldfront/resources`. Install does not generate them.

A saved ZIP, unit choice, MapTiler key, and OSM toggle live in cosmic-config for app id `io.github.drmaas.CosmicColdfront`. The forecast itself is memory only. Restart the applet and open the popup to load weather again.

## Tests

From the repository root:

```bash
just test     # cargo test and the web unit tests
just check    # fmt, clippy -D warnings, tests, applet build, web lint, typecheck, test, and build
```

`cargo fmt` uses a line width of 100 (`rustfmt.toml`).

Shared behavior is locked by `fixtures/contract.json`. The Rust test is `crates/coldfront-core/tests/contract.rs`. The TypeScript test is `apps/coldfront-web/lib/contract.test.ts`. Both read the same JSON files under `fixtures/`.

When a shared rule changes, update the fixture and both tests in one change. `just test` must pass before you open a pull request.

CI on pull requests and on `main` runs web, rust, and secret-scan jobs. A push to `main` that passes those three then runs a deploy job in the same workflow. Manual deploys from your machine still work. CI does not build the Arch package.

## Deploy the web app

### From GitHub Actions

A push to `main` that passes web, rust, and gitleaks deploys the Worker. Pull requests and `workflow_dispatch` do not deploy. Set repository secrets `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID` first. Prefer an Account API token with Workers **Editor** scoped to Worker `coldfront` ([Workers roles](https://developers.cloudflare.com/workers/authorization/workers/)). Legacy Workers Scripts Edit still deploys today; no kill date yet. Then:

```bash
gh secret set CLOUDFLARE_API_TOKEN --repo drmaas/coldfront
gh secret set CLOUDFLARE_ACCOUNT_ID --repo drmaas/coldfront
```

`wrangler-action` receives `accountId` from `CLOUDFLARE_ACCOUNT_ID`. Do not hardcode `account_id` in `wrangler.jsonc`. Keep the API token out of tracked files. The deploy job rebuilds the web app before `wrangler deploy`, same pattern as a second production build after the CI gate.

### From your machine

From `apps/coldfront-web`, copy `.dev.vars.example` to `.dev.vars` and `.env.example` to `.env`. Fill `CLOUDFLARE_ACCOUNT_ID` (and `CLOUDFLARE_API_TOKEN` if you skip interactive login). Wrangler reads the Cloudflare CLI vars from `.env`. Then:

```bash
pnpm exec wrangler secret put MAPTILER_API_KEY
pnpm run build && pnpm exec wrangler deploy
```

Skip the MapTiler secret when the site should serve radar without a street map. The key is not in `wrangler.jsonc`. A missing MapTiler secret must not fail the deploy. Worker secrets survive later deploys.

> [!WARNING]
> Do not put `MAPTILER_API_KEY`, `CLOUDFLARE_API_TOKEN`, or `CLOUDFLARE_ACCOUNT_ID` in source, `wrangler.jsonc`, or any env file you commit.

`compatibility_date` is `2026-11-17`. Workers Logs are enabled. Traces are sampled at 1 percent. The Worker name is `coldfront`. Live URL: `https://coldfront.drmaas.workers.dev`.

## Release the applet

The Arch package release path runs when a `v*` tag is pushed. `.github/workflows/release-arch.yml` runs `scripts/check-version.sh` for that tag, builds `arch/PKGBUILD` in an `archlinux` container, and uploads `cosmic-coldfront-*.pkg.tar.zst` to the GitHub Release. The same workflow also runs on pushes to `main` that touch the applet, workspace crates, or `arch/` (and on `workflow_dispatch`) to warm a shared Cargo cache; those runs skip version check and release upload but still produce an artifact.

Three values must match. The script strips the leading `v` and compares the rest:

- The git tag, for example `v0.2.0`
- `version` in `apps/cosmic-coldfront/Cargo.toml`
- `pkgver` in `arch/PKGBUILD`

`crates/coldfront-core` and `apps/coldfront-web/package.json` are not part of that check.

Check the versions before you push the tag:

```bash
scripts/check-version.sh vX.Y.Z
```

> [!WARNING]
> Do not push a `v*` tag to try the workflow. The workflow publishes a GitHub Release.

To build the package by hand on Arch, from a checkout:

```bash
pkgver=$(sed -n 's/^pkgver=//p' arch/PKGBUILD)
git archive --prefix="coldfront-$pkgver/" -o arch/"coldfront-$pkgver.tar.gz" HEAD
(cd arch && makepkg -si)
```

The archive directory name is `coldfront-$pkgver`, not `cosmic-coldfront-$pkgver`. `sha256sums` is `SKIP` because CI substitutes an archive of the checkout.

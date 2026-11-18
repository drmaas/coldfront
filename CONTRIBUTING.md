# Contributing

This file is how to run Coldfront locally, how to test it, and how to deploy or release it.

Design rules live in [docs/architecture.md](docs/architecture.md). Agent rules live in [AGENTS.md](AGENTS.md).

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

Open `http://localhost:5173`. Leave `MAPTILER_API_KEY` empty to show radar without streets. `.dev.vars` is gitignored. Do not commit it.

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

`just applet install` copies the release binary to `/usr/bin/cosmic-coldfront` and installs the desktop file, metainfo, and icon. That write needs permission for `/usr`. `just applet uninstall` removes those four files.

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

CI on pull requests and on `main` runs three jobs:

- Web: `pnpm install --frozen-lockfile`, Worker types `--check`, lint, typecheck, test, build.
- Rust: fmt, clippy `-D warnings`, `cargo test --workspace --locked`. Clippy and tests already compile the applet, so CI does not run a separate `cargo build`.
- Secret scan: gitleaks.

CI does not deploy the site. CI does not build the Arch package.

## Deploy the web app

Deploy is manual. From `apps/coldfront-web`, after `pnpm exec wrangler login`:

```bash
pnpm exec wrangler secret put MAPTILER_API_KEY
pnpm run build && pnpm exec wrangler deploy
```

Skip the secret when the site should serve radar without a street map. The key is not in `wrangler.jsonc`. A missing secret must not fail the deploy.

> [!WARNING]
> Do not put `MAPTILER_API_KEY` in source, `wrangler.jsonc`, or any env file you commit.

`compatibility_date` is `2026-11-17`. Workers Logs are enabled. Traces are sampled at 1 percent. The Worker name is `coldfront`.

## Release the applet

The Arch package builds only when a `v*` tag is pushed. `.github/workflows/release-arch.yml` runs `scripts/check-version.sh` for that tag, builds `arch/PKGBUILD` in an `archlinux` container, and uploads `cosmic-coldfront-*.pkg.tar.zst` to the GitHub Release.

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

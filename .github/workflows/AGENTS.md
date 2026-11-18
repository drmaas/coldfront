# AGENTS.md

Rules for `.github/workflows`. Also follow the repository [AGENTS.md](../../AGENTS.md). Release steps for people are in [CONTRIBUTING.md](../../CONTRIBUTING.md).

## When to use

Use this file when you add or edit a GitHub Actions workflow.

## When not to use

Do not put product behavior here. Do not deploy the web app from Actions.

## Workflows

`ci.yml` runs on pull requests, pushes to `main`, and `workflow_dispatch`. It has three jobs. None of them publish.

- `web` uses `pnpm/setup@v3` in `apps/coldfront-web` with Node LTS, store cache, and a frozen lockfile. Order is install, `pnpm exec wrangler types --include-runtime false --check`, lint, typecheck, test, build.
- `rust` installs the COSMIC build libraries, then runs fmt, `cargo clippy --all-targets --locked -- -D warnings`, and `cargo test --workspace --locked`. Do not add a separate applet `cargo build` after those steps; clippy and tests already compile the workspace.
- `secrets` runs gitleaks `v8.30.1` with `--redact`. Checkout uses `fetch-depth: 0`.

`release-arch.yml` runs only on a push of a `v*` tag. It needs `permissions: contents: write`. The job uses the `archlinux:latest` container.

## Rules

- Keep CI and release in separate files. Do not build the Arch package on pull requests.
- Do not add a Worker deploy job. Deploy stays manual.
- Keep `ci.yml` free of `contents: write` and of repository secrets.
- Web stays on pnpm and Node LTS (`pnpm/setup` with `runtime: node@lts`). Rust stays on `dtolnay/rust-toolchain@stable` with rustfmt and clippy.
- Cargo invocations in CI stay `--locked`. The PKGBUILD stays `--frozen`.
- Keep the Ubuntu apt mirror rewrite in the `rust` job. The Azure Ubuntu mirror has failed DNS in this workflow. The install step timeout stays 3 minutes.
- Do not drop a library from the `rust` job or the Arch `pacman` line unless the applet builds without it.
- Release must run `scripts/check-version.sh "$GITHUB_REF_NAME"` before `makepkg`. The tag, `apps/cosmic-coldfront/Cargo.toml` `version`, and `arch/PKGBUILD` `pkgver` must match.
- The source archive prefix stays `coldfront-$pkgver/`. Do not name it `cosmic-coldfront-$pkgver/`.
- Run `makepkg` as the unprivileged `builder` user. Do not run it as root.
- `git config --global --add safe.directory` stays. `actions/checkout` inside the container needs it.
- Upload `dist/*.pkg.tar.zst` with `if-no-files-found: error`.
- If the GitHub Release already exists, `gh release upload --clobber`. Otherwise `gh release create` with `--generate-notes`.
- Do not push a `v*` tag to test this workflow. It publishes a release.
- Pin action major versions already in these files (`checkout@v7`, `pnpm/setup@v3`, `upload-artifact@v7`, `Swatinem/rust-cache@v2`). A gitleaks bump changes the download URL and this file together.

## Failures

- `check-version.sh` exits 1 on a version mismatch and exits 2 when the tag argument is missing. The release stops before `makepkg`.
- gitleaks exits non-zero when it finds a secret. Do not add `--no-git` or a skip that hides tracked history.
- A missing `.pkg.tar.zst` fails the artifact upload. Do not set `if-no-files-found` to `warn` or `ignore`.

## Example

Good: add a CI step that still uses `--locked` and keeps clippy `-D warnings`.

Bad: trigger `release-arch.yml` from `pull_request`, or run `pnpm exec wrangler deploy` in `ci.yml`.

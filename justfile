set shell := ["bash", "-cu"]

default:
    @just --list

web *args:
    cd apps/coldfront-web && pnpm {{ args }}

applet *args:
    just --justfile apps/cosmic-coldfront/justfile {{ args }}

fmt:
    cargo fmt --all -- --check
    cd apps/coldfront-web && pnpm run lint

test:
    cargo test --workspace --locked
    cd apps/coldfront-web && pnpm test

check:
    cargo fmt --all -- --check
    cargo clippy --all-targets --locked -- -D warnings
    cargo test --workspace --locked
    cargo build -p cosmic-coldfront --locked
    cd apps/coldfront-web && pnpm exec wrangler types --include-runtime false --check
    cd apps/coldfront-web && pnpm run lint && pnpm run typecheck && pnpm test && pnpm run build

release *args:
    scripts/release.sh {{ args }}

install-applet *args:
    scripts/install-applet.sh {{ args }}

restart-panel *args:
    scripts/restart-panel.sh {{ args }}

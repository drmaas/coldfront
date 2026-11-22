#!/bin/sh
# Release the COSMIC applet: bump versions, commit, push main, push a v* tag.
#
# Usage:
#   scripts/release.sh [--dry-run] [--yes] [--skip-check] <major|minor|patch|X.Y.Z|vX.Y.Z>
#
# Three values must match after the bump (see scripts/check-version.sh):
#   - git tag vX.Y.Z
#   - apps/cosmic-coldfront/Cargo.toml version
#   - arch/PKGBUILD pkgver
#
# The tag push triggers .github/workflows/release-arch.yml, which publishes the
# GitHub Release and the Arch package. Do not push a v* tag only to test that.
set -eu

dry_run=0
assume_yes=0
skip_check=0
bump=

usage() {
    echo "usage: scripts/release.sh [--dry-run] [--yes] [--skip-check] <major|minor|patch|X.Y.Z|vX.Y.Z>" >&2
}

die() {
    echo "$*" >&2
    exit 1
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --dry-run) dry_run=1 ;;
        --yes|-y) assume_yes=1 ;;
        --skip-check) skip_check=1 ;;
        -h|--help)
            usage
            exit 0
            ;;
        -*)
            usage
            die "unknown option: $1"
            ;;
        *)
            if [ -n "$bump" ]; then
                usage
                die "unexpected argument: $1"
            fi
            bump=$1
            ;;
    esac
    shift
done

if [ -z "$bump" ]; then
    usage
    exit 2
fi

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cd "$root"

if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    die "not inside a git repository"
fi

command -v cargo >/dev/null 2>&1 || die "cargo not found"
command -v just >/dev/null 2>&1 || die "just not found"
command -v gh >/dev/null 2>&1 || die "gh not found"

cargo_toml=apps/cosmic-coldfront/Cargo.toml
pkgbuild=arch/PKGBUILD

current=$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$cargo_toml" | head -n 1)
[ -n "$current" ] || die "could not read version from $cargo_toml"

case "$current" in
    *.*.*) ;;
    *) die "current version is not X.Y.Z: $current" ;;
esac

IFS=. read -r cur_major cur_minor cur_patch <<EOF
$current
EOF

case "$bump" in
    major)
        next_major=$((cur_major + 1))
        next="${next_major}.0.0"
        ;;
    minor)
        next_minor=$((cur_minor + 1))
        next="${cur_major}.${next_minor}.0"
        ;;
    patch)
        next_patch=$((cur_patch + 1))
        next="${cur_major}.${cur_minor}.${next_patch}"
        ;;
    v[0-9]*.[0-9]*.[0-9]*|[0-9]*.[0-9]*.[0-9]*)
        next=${bump#v}
        case "$next" in
            [0-9]*.[0-9]*.[0-9]*)
                # Reject extra segments like 1.2.3.4
                case "$next" in
                    *.*.*.*) die "version must be X.Y.Z: $bump" ;;
                esac
                ;;
            *) die "version must be X.Y.Z: $bump" ;;
        esac
        ;;
    *)
        die "expected major, minor, patch, or X.Y.Z; got: $bump"
        ;;
esac

if [ "$next" = "$current" ]; then
    die "version is already $current"
fi

tag="v${next}"

echo "Release plan:"
echo "  current: $current"
echo "  next:    $next ($tag)"
echo "  bump:    $cargo_toml, $pkgbuild, Cargo.lock"
echo "  commit:  chore(release): bump applet to $next"
echo "  push:    origin/main and $tag"
if [ "$skip_check" -eq 1 ]; then
    echo "  check:   skipped"
else
    echo "  check:   just check"
fi

if [ "$dry_run" -eq 1 ]; then
    echo "dry run only; no changes made"
    exit 0
fi

branch=$(git branch --show-current)
[ "$branch" = "main" ] || die "must be on main (on $branch)"

if [ -n "$(git status --porcelain)" ]; then
    die "working tree is not clean"
fi

git fetch origin main --tags

if [ "$(git rev-parse HEAD)" != "$(git rev-parse origin/main)" ]; then
    die "main is not synced with origin/main; pull or push first"
fi

if git rev-parse "$tag" >/dev/null 2>&1; then
    die "local tag already exists: $tag"
fi

if git ls-remote --exit-code --tags origin "refs/tags/$tag" >/dev/null 2>&1; then
    die "remote tag already exists: $tag"
fi

if [ "$assume_yes" -ne 1 ]; then
    printf "Proceed? [y/N] "
    read -r answer
    case "$answer" in
        y|Y|yes|YES) ;;
        *) die "aborted" ;;
    esac
fi

# Replace only the package version line (first ^version = in the crate manifest).
awk -v ver="$next" '
    BEGIN { done = 0 }
    !done && /^version = "/ {
        print "version = \"" ver "\""
        done = 1
        next
    }
    { print }
' "$cargo_toml" >"$cargo_toml.tmp"
mv "$cargo_toml.tmp" "$cargo_toml"

sed -i "s/^pkgver=.*/pkgver=$next/" "$pkgbuild"

cargo update -p cosmic-coldfront --precise "$next"

scripts/check-version.sh "$tag"

restore_web_env() {
    if [ -f apps/coldfront-web/.dev.vars.release-tmp ]; then
        mv apps/coldfront-web/.dev.vars.release-tmp apps/coldfront-web/.dev.vars
    fi
    if [ -f apps/coldfront-web/.env.release-tmp ]; then
        mv apps/coldfront-web/.env.release-tmp apps/coldfront-web/.env
    fi
}

if [ "$skip_check" -eq 0 ]; then
    # Local .dev.vars / .env make `wrangler types --check` disagree with the
    # checked-in empty Env. Hide them for the check, then restore.
    trap restore_web_env EXIT INT TERM
    if [ -f apps/coldfront-web/.dev.vars ]; then
        mv apps/coldfront-web/.dev.vars apps/coldfront-web/.dev.vars.release-tmp
    fi
    if [ -f apps/coldfront-web/.env ]; then
        mv apps/coldfront-web/.env apps/coldfront-web/.env.release-tmp
    fi
    just check
    restore_web_env
    trap - EXIT INT TERM
fi

git add "$cargo_toml" "$pkgbuild" Cargo.lock
git commit -m "chore(release): bump applet to ${next}"

scripts/check-version.sh "$tag"

git push origin main
git tag -a "$tag" -m "$tag"
git push origin "$tag"

echo "Waiting for Release Arch package workflow..."
# Tag push can take a moment to appear in the Actions API.
i=0
run_id=
while [ "$i" -lt 30 ]; do
    run_id=$(gh run list --workflow=release-arch.yml --branch "$tag" --limit 1 --json databaseId --jq '.[0].databaseId // empty')
    if [ -n "$run_id" ]; then
        break
    fi
    i=$((i + 1))
    sleep 2
done

[ -n "$run_id" ] || die "no release-arch run found for $tag"

gh run watch "$run_id" --exit-status
gh release view "$tag"

echo "Released $tag"
echo "  https://github.com/drmaas/coldfront/releases/tag/$tag"

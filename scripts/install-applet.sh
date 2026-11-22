#!/bin/sh
# Install the COSMIC applet from a GitHub Release Arch package.
#
# Usage:
#   scripts/install-applet.sh [--dry-run] [--yes] [latest|X.Y.Z|vX.Y.Z]
#
# Downloads cosmic-coldfront-*-x86_64.pkg.tar.zst from
# https://github.com/drmaas/coldfront/releases and installs it with pacman -U.
# Default version is latest. Needs pacman and curl. Uses gh when available;
# otherwise needs python3 to parse GitHub release JSON.
set -eu

dry_run=0
assume_yes=0
version=
repo=drmaas/coldfront

usage() {
    echo "usage: scripts/install-applet.sh [--dry-run] [--yes] [latest|X.Y.Z|vX.Y.Z]" >&2
}

die() {
    echo "$*" >&2
    exit 1
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --dry-run) dry_run=1 ;;
        --yes|-y) assume_yes=1 ;;
        -h|--help)
            usage
            exit 0
            ;;
        -*)
            usage
            die "unknown option: $1"
            ;;
        *)
            if [ -n "$version" ]; then
                usage
                die "unexpected argument: $1"
            fi
            version=$1
            ;;
    esac
    shift
done

if [ -z "$version" ]; then
    version=latest
fi

case "$version" in
    latest) ;;
    v[0-9]*.[0-9]*.[0-9]*|[0-9]*.[0-9]*.[0-9]*)
        version=${version#v}
        case "$version" in
            *.*.*.*) die "version must be X.Y.Z: $version" ;;
            [0-9]*.[0-9]*.[0-9]*) ;;
            *) die "version must be X.Y.Z: $version" ;;
        esac
        ;;
    *)
        usage
        die "expected latest or X.Y.Z; got: $version"
        ;;
esac

command -v pacman >/dev/null 2>&1 || die "pacman not found; this installer targets Arch Linux"
command -v curl >/dev/null 2>&1 || die "curl not found"

tmpdir=
cleanup() {
    if [ -n "$tmpdir" ] && [ -d "$tmpdir" ]; then
        rm -rf "$tmpdir"
    fi
}
trap cleanup EXIT INT TERM

tmpdir=$(mktemp -d)
cd "$tmpdir"

pick_asset() {
    # stdin: GitHub release JSON. Sets tag, asset, url.
    python3 -c '
import json, re, sys
data = json.load(sys.stdin)
tag = data.get("tag_name") or ""
pat = re.compile(r"^cosmic-coldfront-.*-x86_64\.pkg\.tar\.zst$")
name = ""
for asset in data.get("assets") or []:
    candidate = asset.get("name") or ""
    if pat.match(candidate):
        name = candidate
        break
if not tag or not name:
    sys.exit(1)
print(tag)
print(name)
'
}

resolve_release() {
    if command -v gh >/dev/null 2>&1; then
        if [ "$version" = "latest" ]; then
            tag=$(gh release view --repo "$repo" --json tagName --jq .tagName)
        else
            tag="v${version}"
            gh release view --repo "$repo" "$tag" >/dev/null
        fi
        asset=$(gh release view --repo "$repo" "$tag" --json assets --jq \
            '[.assets[].name | select(test("^cosmic-coldfront-.*-x86_64\\.pkg\\.tar\\.zst$"))] | first // empty')
        [ -n "$asset" ] || die "no Arch package asset on $tag"
    else
        command -v python3 >/dev/null 2>&1 || die "need gh or python3 to parse GitHub release JSON"
        if [ "$version" = "latest" ]; then
            api_url="https://api.github.com/repos/${repo}/releases/latest"
        else
            api_url="https://api.github.com/repos/${repo}/releases/tags/v${version}"
        fi
        curl -fsSL -H 'Accept: application/vnd.github+json' "$api_url" -o release.json || \
            die "failed to fetch release metadata from GitHub"
        resolved=$(pick_asset <release.json) || die "no Arch package asset on that release"
        tag=$(printf '%s\n' "$resolved" | sed -n '1p')
        asset=$(printf '%s\n' "$resolved" | sed -n '2p')
    fi
    url="https://github.com/${repo}/releases/download/${tag}/${asset}"
}

resolve_release

echo "Install plan:"
echo "  release: $tag"
echo "  asset:   $asset"
echo "  url:     $url"
echo "  install: sudo pacman -U ./$asset"

if [ "$dry_run" -eq 1 ]; then
    echo "dry run only; no download or install"
    exit 0
fi

if [ "$assume_yes" -ne 1 ]; then
    printf "Proceed? [y/N] "
    read -r answer
    case "$answer" in
        y|Y|yes|YES) ;;
        *) die "aborted" ;;
    esac
fi

echo "Downloading $asset..."
curl -fsSL -o "$asset" -L "$url" || die "download failed: $url"
[ -s "$asset" ] || die "downloaded file is empty: $asset"

if [ "$assume_yes" -eq 1 ]; then
    sudo pacman -U --noconfirm "./$asset"
else
    sudo pacman -U "./$asset"
fi

echo "Installed $tag ($asset)"
echo "Restart the Coldfront applet in the COSMIC panel to load the new binary."

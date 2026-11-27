#!/bin/sh
# Restart the COSMIC panel so it relaunches its applets from the installed binary.
#
# Usage:
#   scripts/restart-panel.sh [--dry-run] [--timeout SECONDS]
#
# cosmic-panel is a child of cosmic-session, not a systemd user unit. Sending it
# SIGTERM makes cosmic-session respawn it, and every panel applet is relaunched
# as a child of the new panel. That is how a freshly installed
# /usr/bin/cosmic-coldfront replaces the copy that is already running.
#
# If cosmic-session does not respawn the panel, this script starts cosmic-panel
# itself with the session environment copied from cosmic-session.
set -eu

dry_run=0
timeout=15

panel_name=cosmic-panel
session_name=cosmic-session

usage() {
    echo "usage: scripts/restart-panel.sh [--dry-run] [--timeout SECONDS]" >&2
}

die() {
    echo "$*" >&2
    exit 1
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --dry-run) dry_run=1 ;;
        --timeout)
            [ "$#" -ge 2 ] || {
                usage
                die "--timeout needs a value"
            }
            timeout=$2
            shift
            ;;
        --timeout=*) timeout=${1#*=} ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            usage
            die "unknown argument: $1"
            ;;
    esac
    shift
done

case "$timeout" in
    ''|*[!0-9]*|0) die "timeout must be a positive whole number of seconds: $timeout" ;;
esac

command -v pgrep >/dev/null 2>&1 || die "pgrep not found; install procps-ng"

panel_pid=$(pgrep -x "$panel_name" | head -n 1 || true)
session_pid=$(pgrep -x "$session_name" | head -n 1 || true)

echo "Restart plan:"
echo "  panel:   ${panel_pid:-not running}"
echo "  session: ${session_pid:-not found}"
echo "  action:  SIGTERM $panel_name and wait up to ${timeout}s for $session_name to respawn it"

if [ "$dry_run" -eq 1 ]; then
    echo "dry run only; no changes made"
    exit 0
fi

# Print the pid of a panel process that is not the one named in $1, or nothing.
wait_for_panel() {
    skip=$1
    tries=$((timeout * 4))
    i=0
    while [ "$i" -lt "$tries" ]; do
        found=$(pgrep -x "$panel_name" | grep -v "^${skip:-unset}$" | head -n 1 || true)
        if [ -n "$found" ]; then
            echo "$found"
            return 0
        fi
        i=$((i + 1))
        sleep 0.25
    done
    return 1
}

# Start cosmic-panel from the session environment when cosmic-session did not
# respawn it. Only the variables Wayland, D-Bus, and the panel itself need are
# copied; the session environment also holds unrelated per-child values.
launch_from_session() {
    [ -n "$session_pid" ] || return 1
    [ -r "/proc/$session_pid/environ" ] || return 1
    env_file=$(mktemp) || return 1
    if ! tr '\0' '\n' <"/proc/$session_pid/environ" |
        grep -E '^(DBUS_SESSION_BUS_ADDRESS|WAYLAND_DISPLAY|DISPLAY|XDG_RUNTIME_DIR|XDG_SESSION_TYPE|XDG_CURRENT_DESKTOP|XDG_DATA_DIRS|PATH)=' >"$env_file"; then
        rm -f "$env_file"
        return 1
    fi
    while IFS='=' read -r key value; do
        [ -n "$key" ] || continue
        export "$key=$value"
    done <"$env_file"
    rm -f "$env_file"
    [ -n "${WAYLAND_DISPLAY:-}${DISPLAY:-}" ] || return 1
    setsid -f "$panel_name" >/dev/null 2>&1 || return 1
    return 0
}

if [ -n "$panel_pid" ]; then
    kill -TERM "$panel_pid" 2>/dev/null || die "could not signal $panel_name (pid $panel_pid)"
    new_pid=$(wait_for_panel "$panel_pid" || true)
else
    echo "$panel_name is not running; starting it"
    new_pid=
fi

if [ -z "$new_pid" ]; then
    echo "$session_name did not respawn $panel_name; starting it from the session environment" >&2
    launch_from_session || die "could not start $panel_name"
    new_pid=$(wait_for_panel "" || true)
fi

[ -n "$new_pid" ] || die "$panel_name did not come back; restart the session or run $panel_name by hand"

# Give the panel a moment to spawn its applet children before reporting.
# The applet name is 16 characters, so pgrep -x would miss it and a plain -f
# would match this script's own command line. Anchor on the program name.
sleep 1
applet_pid=$(pgrep -f '^cosmic-coldfront( |$)' | head -n 1 || true)

echo "Restarted $panel_name (pid $new_pid)"
if [ -n "$applet_pid" ]; then
    echo "  cosmic-coldfront relaunched (pid $applet_pid)"
else
    echo "  cosmic-coldfront is not running; open the applet from the panel"
fi

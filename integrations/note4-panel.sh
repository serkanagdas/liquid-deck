#!/usr/bin/env bash
#
# Liquid Deck — Galaxy Note 4 panel helper.
#
# Wakes, sleeps and inspects an Android tablet over ADB. Intended for a
# Galaxy Note 4 (SM-N910C) used as an always-on dashboard next to a PC.
#
#   ./note4-panel.sh wake       wake, dismiss keyguard, open DECK_URL
#   ./note4-panel.sh sleep      press the power/sleep key
#   ./note4-panel.sh status     model, Android version, battery, screen state
#   ./note4-panel.sh watch      keep awake while /api/health reports clients
#   ./note4-panel.sh stayon     keep the screen on while charging
#   ./note4-panel.sh stayoff    restore the normal screen timeout
#
# Environment:
#   ADB_BIN=adb
#   ADB_TARGET=192.168.1.101:5555
#   DECK_URL=http://192.168.1.50:8787/   (host LAN/Tailscale URL)
#   BROWSER_COMPONENT=com.android.chrome
#   HEALTH_URL=http://127.0.0.1:8787/api/health
#   POLL_SECS=5   IDLE_SECS=90   OPEN_BROWSER=1
#
set -euo pipefail

ADB_BIN=${ADB_BIN:-adb}
ADB_TARGET=${ADB_TARGET:-192.168.1.101:5555}
DECK_URL=${DECK_URL:-}
BROWSER_COMPONENT=${BROWSER_COMPONENT:-com.android.chrome}
HEALTH_URL=${HEALTH_URL:-http://127.0.0.1:8787/api/health}
POLL_SECS=${POLL_SECS:-5}
IDLE_SECS=${IDLE_SECS:-90}
OPEN_BROWSER=${OPEN_BROWSER:-1}

usage() {
    cat <<'EOF'
Liquid Deck — Galaxy Note 4 panel helper.

Usage:
  note4-panel.sh wake       wake, dismiss keyguard, open DECK_URL
  note4-panel.sh sleep      press the power/sleep key
  note4-panel.sh status     model, Android version, battery, screen state
  note4-panel.sh watch      keep awake while /api/health reports clients
  note4-panel.sh stayon     keep the screen on while charging
  note4-panel.sh stayoff    restore the normal screen timeout

Environment:
  ADB_BIN=adb
  ADB_TARGET=192.168.1.101:5555
  DECK_URL=http://192.168.1.50:8787/   (host LAN/Tailscale URL)
  BROWSER_COMPONENT=com.android.chrome
  HEALTH_URL=http://127.0.0.1:8787/api/health
  POLL_SECS=5   IDLE_SECS=90   OPEN_BROWSER=1
EOF
}

adb_device() {
    "$ADB_BIN" -s "$ADB_TARGET" "$@"
}

connect() {
    "$ADB_BIN" connect "$ADB_TARGET" >/dev/null 2>&1 || true
    for _ in 1 2 3 4 5; do
        if [ "$("$ADB_BIN" -s "$ADB_TARGET" get-state 2>/dev/null || true)" = "device" ]; then
            return 0
        fi
        sleep 1
    done
    echo "note4-panel: cannot reach $ADB_TARGET" >&2
    echo "note4-panel: run 'adb connect $ADB_TARGET' and accept the RSA prompt." >&2
    return 1
}

screen_awake() {
    adb_device shell dumpsys power 2>/dev/null |
        grep -qE 'mWakefulness=Awake|Display Power: state=ON'
}

wake() {
    connect

    if ! screen_awake; then
        adb_device shell input keyevent KEYCODE_WAKEUP >/dev/null
        sleep 0.4
        adb_device shell wm dismiss-keyguard >/dev/null 2>&1 || true
    fi

    if [ "$OPEN_BROWSER" = "1" ]; then
        if [ -z "$DECK_URL" ]; then
            echo "note4-panel: DECK_URL is empty; screen woken without opening a browser." >&2
        elif ! adb_device shell am start -a android.intent.action.VIEW \
            -d "$DECK_URL" "$BROWSER_COMPONENT" >/dev/null 2>&1; then
            adb_device shell am start -a android.intent.action.VIEW \
                -d "$DECK_URL" >/dev/null 2>&1 || true
        fi
    fi

    echo "awake"
}

sleep_device() {
    connect
    adb_device shell input keyevent KEYCODE_SLEEP >/dev/null
    echo "asleep"
}

status() {
    connect
    echo "target:   $ADB_TARGET"
    printf 'model:    %s\n' "$(adb_device shell getprop ro.product.model | tr -d '\r')"
    printf 'android:  %s\n' "$(adb_device shell getprop ro.build.version.release | tr -d '\r')"
    adb_device shell dumpsys battery |
        grep -E '^  (level|status|temperature):' |
        tr -d '\r' |
        sed 's/^  /battery:  /'
    if screen_awake; then
        echo "screen:   awake"
    else
        echo "screen:   asleep"
    fi
}

stayon() {
    connect
    # Android only honours this while the device is charging.
    adb_device shell svc power stayon true >/dev/null
    echo "stay-on enabled (while charging)"
}

stayoff() {
    connect
    adb_device shell svc power stayon false >/dev/null
    echo "stay-on disabled"
}

watch() {
    command -v curl >/dev/null 2>&1 || {
        echo "note4-panel: curl is required for watch mode" >&2
        exit 1
    }

    local last_active awake
    last_active=$(date +%s)
    awake=0
    if screen_awake 2>/dev/null; then
        awake=1
    fi

    echo "watching $HEALTH_URL every ${POLL_SECS}s (idle ${IDLE_SECS}s)"
    echo "note: a browser that keeps its WebSocket open while the screen is off"
    echo "      can keep 'clients' above zero; prefer lifecycle hooks or cron."

    while :; do
        local clients
        clients=$(curl -fsS --max-time 3 "$HEALTH_URL" 2>/dev/null |
            sed -n 's/.*"clients":\([0-9][0-9]*\).*/\1/p' || true)

        if [ -n "$clients" ] && [ "$clients" -gt 0 ] 2>/dev/null; then
            last_active=$(date +%s)
            if [ "$awake" -eq 0 ]; then
                wake || true
                awake=1
            fi
        else
            local now idle
            now=$(date +%s)
            idle=$((now - last_active))
            if [ "$awake" -eq 1 ] && [ "$idle" -ge "$IDLE_SECS" ]; then
                sleep_device || true
                awake=0
            fi
        fi

        sleep "$POLL_SECS"
    done
}

case "${1:-}" in
    wake) wake ;;
    sleep) sleep_device ;;
    status) status ;;
    watch) watch ;;
    stayon) stayon ;;
    stayoff) stayoff ;;
    -h | --help | help) usage ;;
    *)
        usage >&2
        exit 2
        ;;
esac

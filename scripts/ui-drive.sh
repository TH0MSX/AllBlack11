#!/usr/bin/env bash
# Drive the real Settings window under Wine with synthetic clicks and take screenshots.
#
#   scripts/ui-drive.sh PAGE "click X Y" "shot /tmp/a.png" "key ctrl+k" "wait 1" ...
#
# Same caveats as ui-shot.sh: Wine, not Windows. Coordinates are window-relative pixels.
set -euo pipefail
page="$1"; shift
exe="$(dirname "$0")/../target/x86_64-pc-windows-msvc/release/AllBlack11.exe"
export WINEPREFIX="${WINEPREFIX:-/tmp/wineprefix}" WINEDEBUG=-all DISPLAY="${DISPLAY:-:99}"
# Wine's explorer.exe draws the system tray and balloon notifications as top-level windows that
# float over the capture. They are not part of AllBlack11; hide them so screenshots show only it.
unmap_wine_shell() { for w in $(xdotool search --onlyvisible --class 'explorer.exe' 2>/dev/null); do xdotool windowunmap "$w" 2>/dev/null || true; done; }
pgrep -x Xvfb >/dev/null || { Xvfb "$DISPLAY" -screen 0 2000x1400x24 >/dev/null 2>&1 & sleep 2; }
/usr/lib/wine/wineserver -k 2>/dev/null || true; sleep 1
/usr/lib/wine/wine64 "$exe" --open-settings="$page" >/tmp/ab11-wine.log 2>&1 &
for _ in $(seq 1 40); do
  id="$(xdotool search --name 'AllBlack11 Settings' 2>/dev/null | head -1 || true)"; [ -n "$id" ] && break; sleep 0.5
done
[ -n "${id:-}" ] || { echo "no window" >&2; exit 1; }
xdotool windowmove "$id" 0 0; xdotool windowsize "$id" 1100 800; sleep 2
for action in "$@"; do
  set -- $action
  case "$1" in
    click) xdotool mousemove "$2" "$3"; sleep 0.2; xdotool click 1; sleep 0.8 ;;
    key) xdotool key --window "$id" "$2"; sleep 0.8 ;;
    type) xdotool type --delay 40 "$2"; sleep 0.5 ;;
    wait) sleep "$2" ;;
    shot) unmap_wine_shell; xdotool windowraise "$id"; sleep 0.3; import -window "$id" "$2" ;;
  esac
done
/usr/lib/wine/wineserver -k 2>/dev/null || true

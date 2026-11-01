#!/usr/bin/env bash
# Render the real AllBlack11 Settings window under Wine + Xvfb and save a screenshot.
#
#   scripts/ui-shot.sh PAGE OUT.png [WIDTHxHEIGHT]
#
# This is a LAYOUT aid only. Wine is not Windows: fonts fall back (no Segoe UI Variable),
# DWM attributes, dark title bars, Mica, DPI and UI Automation behave differently or are
# absent. A screenshot from here proves the layout code runs and where text lands; it does
# not prove how Windows will render it.
#
# Needs: wine64 (/usr/lib/wine/wine64), Xvfb, imagemagick, xdotool, the x64 release build:
#   cargo xwin build --release --target x86_64-pc-windows-msvc
set -euo pipefail
page="${1:-0}"; out="${2:-/tmp/ui-shot.png}"; size="${3:-1000x760}"
exe="$(dirname "$0")/../target/x86_64-pc-windows-msvc/release/AllBlack11.exe"
export WINEPREFIX="${WINEPREFIX:-/tmp/wineprefix}" WINEDEBUG=-all DISPLAY="${DISPLAY:-:99}"
# Wine's explorer.exe draws the system tray and balloon notifications as top-level windows that
# float over the capture. They are not part of AllBlack11; hide them so screenshots show only it.
unmap_wine_shell() { for w in $(xdotool search --onlyvisible --class 'explorer.exe' 2>/dev/null); do xdotool windowunmap "$w" 2>/dev/null || true; done; }
pgrep -x Xvfb >/dev/null || { Xvfb "$DISPLAY" -screen 0 2000x1400x24 >/dev/null 2>&1 & sleep 2; }
/usr/lib/wine/wineserver -k 2>/dev/null || true
sleep 1
rm -rf "$WINEPREFIX/drive_c/users/root/AppData/Roaming/AllBlack11/session.json"
/usr/lib/wine/wine64 "$exe" --open-settings="$page" >/tmp/ab11-wine.log 2>&1 &
for _ in $(seq 1 40); do
  id="$(xdotool search --name 'AllBlack11 Settings' 2>/dev/null | head -1 || true)"
  [ -n "$id" ] && break; sleep 0.5
done
[ -n "${id:-}" ] || { echo "Settings window did not appear" >&2; exit 1; }
xdotool windowmove "$id" 0 0 2>/dev/null || true
xdotool windowsize "$id" "${size%x*}" "${size#*x}" 2>/dev/null || true
sleep 2
unmap_wine_shell; xdotool windowraise "$id" 2>/dev/null || true; sleep 0.3
import -window "$id" "$out"
/usr/lib/wine/wineserver -k 2>/dev/null || true
echo "saved $out"

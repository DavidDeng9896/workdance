#!/usr/bin/env bash
# Finalize + verify OpenLess.app (macOS CI): vendor non-system deps of
# libmediapipe into Frameworks, enforce @rpath id, ad-hoc sign whole .app,
# verify, then build the .dmg from the signed .app. Fails on any violation.
# Usage: mac-finalize-app.sh <path/to/OpenLess.app> <out.dmg>
set -euo pipefail
APP="$1"; DMG="$2"
FW="$APP/Contents/Frameworks"; LIB="$FW/libmediapipe.dylib"
MODEL="$APP/Contents/Resources/models/hand_landmarker.task"
test -f "$LIB" || { echo "FAIL: missing $LIB" >&2; exit 1; }
test -f "$MODEL" || { echo "FAIL: missing $MODEL" >&2; exit 1; }
chmod -R u+w "$FW"

is_ok_dep() { [[ "$1" == /usr/lib/* || "$1" == /System/* || "$1" == @rpath/* || "$1" == @loader_path/* || "$1" == @executable_path/../Frameworks/* ]]; }
deps() { otool -L "$1" | tail -n +2 | awk '{print $1}' | grep -vx "$(otool -D "$1" | tail -n +2)" || true; }

# Vendor any non-system dep (one level per pass, repeat until stable).
for pass in 1 2 3 4 5; do
  changed=0
  for f in "$FW"/*.dylib; do
    for d in $(deps "$f"); do
      is_ok_dep "$d" && continue
      [[ -f "$d" ]] || { echo "FAIL: $f needs $d (not found on runner)" >&2; exit 1; }
      b=$(basename "$d")
      [[ -f "$FW/$b" ]] || { cp "$d" "$FW/$b"; chmod u+w "$FW/$b"; install_name_tool -id "@rpath/$b" "$FW/$b"; }
      install_name_tool -change "$d" "@loader_path/$b" "$f"
      changed=1
    done
  done
  [[ $changed == 0 ]] && break
done

install_name_tool -id @rpath/libmediapipe.dylib "$LIB"

# Gate 1: only system / relative deps.
bad=0
for f in "$FW"/*.dylib; do
  echo "== otool -L $f"; otool -L "$f"
  for d in $(deps "$f"); do is_ok_dep "$d" || { echo "FAIL: non-system dep $d in $f" >&2; bad=1; }; done
done
[[ $bad == 0 ]] || exit 1
# Gate 2: id.
id=$(otool -D "$LIB" | tail -n +2)
[[ "$id" == "@rpath/libmediapipe.dylib" ]] || { echo "FAIL: id=$id" >&2; exit 1; }
# Gate: symbols + arch.
# Write nm to a file first: `nm | grep -q` under pipefail exits 141 (SIGPIPE) on a hit.
nm -gU "$LIB" > /tmp/mp-nm.txt 2>/tmp/mp-nm.err || true
if ! grep -F _MpHandLandmarkerCreate /tmp/mp-nm.txt >/dev/null; then
  # Stripped / hidden visibility: fall back to strings.
  if ! strings -a "$LIB" | grep -F _MpHandLandmarkerCreate >/dev/null; then
    echo "FAIL: no _MpHandLandmarkerCreate" >&2
    head -20 /tmp/mp-nm.err >&2 || true
    exit 1
  fi
fi
for f in "$LIB" "$APP/Contents/MacOS/"*; do
  a=$(lipo -archs "$f"); echo "lipo $f: $a"
  [[ " $a " == *" arm64 "* ]] || { echo "FAIL: $f not arm64" >&2; exit 1; }
done
# Gate 3: ad-hoc sign whole bundle after all files placed, then verify.
codesign --force --deep -s - "$APP"
codesign -v --deep --strict --verbose=2 "$APP"
ls -la "$FW" "$(dirname "$MODEL")"

# DMG from the signed .app (+ /Applications link).
STAGE=$(mktemp -d); cp -R "$APP" "$STAGE/"; ln -s /Applications "$STAGE/Applications"
rm -f "$DMG"
hdiutil create -volname OpenLess -srcfolder "$STAGE" -ov -format UDZO "$DMG"
hdiutil verify "$DMG"
# Re-check signature inside the dmg.
MNT=$(mktemp -d); hdiutil attach -nobrowse -readonly -mountpoint "$MNT" "$DMG" >/dev/null
codesign -v --deep --strict "$MNT/$(basename "$APP")"; test -f "$MNT/$(basename "$APP")/Contents/Frameworks/libmediapipe.dylib"
hdiutil detach "$MNT" >/dev/null
ls -la "$DMG"

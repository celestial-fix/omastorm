#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
check_dir="$PWD/target/check-map-hazards"
mkdir -p "$check_dir" review
rm -f review/hazard-tooltip.png
cp ui/RadarMap.qml "$check_dir/"
cp tests/map-hazards.qml "$check_dir/shell.qml"
ln -sfn "$PWD/ui/shaders" "$check_dir/shaders"
QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=basic QT_QUICK_BACKEND=rhi QSG_RHI_BACKEND=opengl \
 OMASTORM_REVIEW="$PWD/review" timeout 15 quickshell -p "$check_dir/shell.qml" > "$check_dir/result.log" 2>&1
cat "$check_dir/result.log"
rg -q MAP_HAZARDS_PASSED "$check_dir/result.log"
test -s review/hazard-tooltip.png
if rg -q 'TypeError|ReferenceError|Unable to assign|Failed to create.*context' "$check_dir/result.log"; then exit 1; fi

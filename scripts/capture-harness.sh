#!/usr/bin/env bash
# The harness shell for captures of states the real feed cannot be asked
# for: the real UI files, and an Engine that lays OMASTORM_STATE_OVERRIDE (a
# JSON object) over every state it receives, one level deep, so a `frame`
# or `connection` field can change while the rest stays real. Prints the
# shell's path for OMASTORM_QML.
set -euo pipefail
cd "$(dirname "$0")/.."
harness="$PWD/target/capture-harness"
mkdir -p "$harness"
cp ui/shell.qml ui/RadarWindow.qml ui/RadarMark.qml ui/RadarMap.qml ui/SitePicker.qml ui/Sites.js ui/KeysSheet.qml ui/Keys.js ui/Timeline.js ui/Theme.qml ui/Config.qml ui/Toml.js ui/Location.js ui/LocationPicker.qml ui/WeatherPicker.qml ui/Remembered.qml ui/PluginSession.qml ui/qmldir "$harness/"
ln -sfn "$PWD/ui/shaders" "$harness/shaders"
perl -pe 's/^(\s+)state = message;$/$1var override = JSON.parse(Quickshell.env("OMASTORM_STATE_OVERRIDE") || "{}");\n$1for (var key in override) message[key] = override[key] && typeof override[key] === "object" && !Array.isArray(override[key]) && message[key] ? Object.assign(message[key], override[key]) : override[key];\n$1state = message;/' ui/Engine.qml > "$harness/Engine.qml"
grep -q OMASTORM_STATE_OVERRIDE "$harness/Engine.qml"
echo "$harness/shell.qml"

#!/usr/bin/env bash
# Weather location file (DESIGN.md, location): Omarchy weather.json and
# WeeWX / WeatherAPI / OpenWeatherMap / Tomorrow.io / Visual Crossing
# payloads with coordinates become a place; a name alone does not.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p review
root="$PWD"
tmp=$(mktemp -d "$PWD/review/weather-location-check.XXXXXX")
trap 'if [[ -n ${pid:-} ]]; then kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true; fi; rm -rf "$tmp"' EXIT
mkdir -p "$tmp/ui"
cp ui/Config.qml ui/Location.js ui/Toml.js "$tmp/ui/"
printf '%s\n' '{"name":"Stokesdale","latitude":36.23708,"longitude":-79.97948}' > "$tmp/weather.json"
cat > "$tmp/ui/shell.qml" <<'QML'
import Quickshell
import Quickshell.Io
import "Location.js" as Location
ShellRoot {
    Config { id: config }
    IpcHandler {
        target: "check"
        function parse(raw: string): string {
            var loc = Location.parseWeatherLocation(raw);
            return loc ? JSON.stringify(loc) : "null";
        }
        function fileLocation(): string {
            return config.location ? JSON.stringify(config.location) : "null";
        }
        function weather(configRaw: string, fileRaw: string): string {
            var cfg = configRaw ? JSON.parse(configRaw) : {};
            var file = fileRaw ? JSON.parse(fileRaw) : {};
            return JSON.stringify(Location.weatherSettings(cfg, file));
        }
        function line(raw: string): string {
            return Location.formatWeather(JSON.parse(raw));
        }
    }
}
QML
export QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=basic
export OMASTORM_CONFIG="$tmp/missing.toml" OMASTORM_LOCATION="$tmp/weather.json"
quickshell -p "$tmp/ui/shell.qml" > "$tmp/log" 2>&1 &
pid=$!
call() { # method, args...
    quickshell ipc --pid "$pid" call check "$@" 2>/dev/null || true
}
expect_parse() { # raw json, expected snapshot
    local actual=""
    for _ in {1..50}; do
        actual=$(call parse "$1")
        if [[ "$actual" == "$2" ]]; then return; fi
        sleep .1
    done
    printf 'parse\nInput: %s\nExpected: %s\nActual: %s\n' "$1" "$2" "$actual" >&2
    cat "$tmp/log" >&2
    exit 1
}
expect_file() { # expected snapshot
    local actual=""
    for _ in {1..50}; do
        actual=$(call fileLocation)
        if [[ "$actual" == "$1" ]]; then return; fi
        sleep .1
    done
    printf 'file\nExpected: %s\nActual: %s\n' "$1" "$actual" >&2
    cat "$tmp/log" >&2
    exit 1
}

stokes='{"name":"Stokesdale","lat":36.23708,"lon":-79.97948}'
expect_file "$stokes"
expect_parse '{"name":"Stokesdale","latitude":36.23708,"longitude":-79.97948}' "$stokes"
expect_parse '{"name":"Malibu"}' null
expect_parse '{"latitude":91,"longitude":0}' null
expect_parse '{"station":{"location":"Stokesdale","latitude":"36.23708","longitude":"-79.97948"}}' "$stokes"
expect_parse '{"location":{"name":"Stokesdale","lat":36.23708,"lon":-79.97948},"current":{"temp_c":22}}' "$stokes"
expect_parse '{"coord":{"lon":-79.97948,"lat":36.23708},"name":"Stokesdale"}' "$stokes"
expect_parse '{"lat":36.23708,"lon":-79.97948}' '{"name":"","lat":36.23708,"lon":-79.97948}'
expect_parse '{"data":{"values":{"temperature":22}},"location":{"lat":36.23708,"lon":-79.97948,"name":"Stokesdale"}}' "$stokes"
expect_parse '{"latitude":36.23708,"longitude":-79.97948,"resolvedAddress":"Stokesdale, NC, United States","address":"Stokesdale"}' '{"name":"Stokesdale, NC, United States","lat":36.23708,"lon":-79.97948}'
expect_parse '{"type":"FeatureCollection","features":[{"type":"Feature","geometry":{"type":"Point","coordinates":[-79.97948,36.23708]},"properties":{"name":"Stokesdale"}}]}' "$stokes"

printf '%s\n' '{"location":{"name":"Moore","lat":35.4,"lon":-97.5}}' > "$tmp/weather.json"
expect_file '{"name":"Moore","lat":35.4,"lon":-97.5}'

expect_weather() {
    local actual=""
    for _ in {1..50}; do
        actual=$(call weather "$1" "$2")
        if [[ "$actual" == "$3" ]]; then return; fi
        sleep .1
    done
    printf 'weatherSettings\nConfig: %s\nFile: %s\nExpected: %s\nActual: %s\n' "$1" "$2" "$3" "$actual" >&2
    exit 1
}
expect_weather '{}' '{}' '{"source":"","apiKey":"","url":"","errors":[]}'
expect_weather '{"weather.source":"openweathermap","weather.api_key":"abc"}' '{}' '{"source":"openweathermap","apiKey":"abc","url":"","errors":[]}'
expect_weather '{"weather.source":"openweathermap"}' '{}' '{"source":"","apiKey":"","url":"","errors":["weather.api_key is required for openweathermap"]}'
expect_weather '{"weather.source":"weewx","weather.url":"http://192.168.1.8/data.json"}' '{}' '{"source":"weewx","apiKey":"","url":"http://192.168.1.8/data.json","errors":[]}'
expect_weather '{}' '{"source":"tomorrow","api_key":"tok"}' '{"source":"tomorrow","apiKey":"tok","url":"","errors":[]}'

line=$(call line '{"status":"ok","temperatureC":22.1,"condition":"Overcast","sourceName":"OpenWeatherMap","observedAt":"2023-09-10T16:00:00Z"}')
[[ "$line" == *'22°C'* && "$line" == *'72°F'* && "$line" == *'OVERCAST'* && "$line" == *'OPENWEATHERMAP'* ]] \
  || { printf 'formatWeather: %s\n' "$line" >&2; exit 1; }

printf 'Weather location Omarchy file, WeeWX, and weather-API payloads: PASS\n'

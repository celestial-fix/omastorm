# Omastorm design

How a change, feature, or fix should behave.

[README.md](README.md) is install and use. [docs/protocol.md](docs/protocol.md)
is the wire. [docs/configuration.md](docs/configuration.md) is the config keys.
[CONTRIBUTING.md](CONTRIBUTING.md) is the contribution workflow. Honor these; ask before
violating them.

## Picture

Weather occupies the view. Geography stays quiet: thin lines, sparse labels,
rings, a crosshair. Chrome follows the Omarchy theme; radar color comes only
from `frame.palette`, shared by the legend and the shader.

Pixels, Glyphs, and Stipple all stay. They sample the same cell and palette
and differ only in how the 3 px cell is painted. Glyphs is the default. A
shader or sampling change updates all three and is shown with a capture, not
described.

Show actual scan times. Label archived data. Missing, range-folded, and
below-threshold stay distinct from measured values. Whatever a change hides
is named in the legend. Keep OSM (ODbL) and Natural Earth attribution with
the data and on screen.

## Location, onboarding, and map

Map center and radar source are independent. The center is the place the
user wants to see; the radar supplies one station's exact sweep. Loading a
frame or handing off to another station never moves the camera.

After the plugin ensures the engine is available and starts it, resolve the
map center in this order:

1. Explicit `center_lat` and `center_lon` in `config.toml`.
2. The last center remembered in `state.json`.
3. Valid coordinates from the weather location file (`weather.json`):
   Omarchy's `{name, latitude, longitude}`, or the same place fields in a
   WeeWX, WeatherAPI, OpenWeatherMap, Tomorrow.io, or Visual Crossing
   payload. Do not fetch those APIs to discover a place.
4. A location chosen through Omastorm's location picker.

Only show onboarding when none of the first three sources supplies a valid
center. The popover offers "Choose a location", opening the expanded
window's picker. Offer place search and "Enter coordinates", which reveals
labeled latitude and longitude fields with validation. Place search is an
engine `search_places` reply over GeoNames cities with population ≥ 5000
worldwide (state/region and country so two Jacksonvilles are
distinct); map labels stay Natural Earth. "Show radar" accepts the location.
No separate setup wizard or settings window is required. Keep the picker
reachable after onboarding (`Shift+H` and LOCATION). Coordinate entry
chooses a view; it does not create a permanent config override or lock a
radar. Choosing a location writes `state.json`, never `config.toml`.

Reuse Omarchy's location when available without requiring its weather plugin.
Read weather settings only; never write them. Location search is an explicit
user action handled through the engine. Do not use GeoClue or fetch at launch
to discover the user's location.

Resolve the radar separately: an explicit `locked_radar` in config wins,
otherwise restore a remembered radar lock, otherwise choose the station
nearest the map center when it lies inside the 460 km reflectivity
footprint. Outside that reach the map stands without a sweep; do not borrow
a distant NEXRAD site. A radar lock alone does not supply a map center or
bypass location onboarding. Newly chosen locations start unlocked unless a
configured radar override applies.

Remember center and zoom after movement settles, and remember changes to the
UI radar lock. Unlocked radar selection follows the center using the protocol's
nearest-station hysteresis; do not wait until the center leaves the radar's
rings. Lock pins the source; `n` releases it and selects the nearest station
without moving the camera. Choosing a station in search locks it and centres
the map on that site. Automatic hand-off and loading a frame never move the
camera. Do not persist the automatically selected station.

Closing preserves the view. Reopening restores it, with explicit config
values taking precedence. Expanding the popover preserves its center, zoom,
station, frame, and playback. An engine reconnect restores the necessary
commands without resetting the user's camera. Weather location supplies an
initial view; subsequent weather changes do not overwrite a remembered view.

Explicit coordinates are honored on every launch and do not imply a radar
lock. A configured center far from a locked radar is valid: preserve both,
show the station and lock clearly, and offer "Use nearest radar" and "Go to
selected radar" when that radar's coverage is outside the view. UI navigation
and unlocking can change the active session; explicit config applies again
on launch.

A station with no frame yet is the map without radar. Show no loading animation.
Display one radar station’s sweep at a time.

## Split

The engine fetches, decodes, caches, and rasterizes. The UI is small state
plus GPU textures. Radar values do not enter JSON or QML. Pan and zoom are
uniforms. The engine reads neither `config.toml` nor `state.json`; the UI
resolves preferences and remembered state, then sends commands.

New settings are optional, omit means default, and a bad value is named in
the status slot. Keep deliberate settings in `config.toml` and session restore in `state.json`.
The app never rewrites config because the user pans, zooms, or changes a lock.
Write `state.json` atomically. See [configuration](docs/configuration.md) for
file ownership and precedence. Do not write Omarchy, Hyprland, or system
configuration.

A product is a texture, legend, units, timestamp, and source from the engine.
Level II reflectivity is the radar layer (a report). Live mode also offers
field layers — wind, pressure, atmospheric water, temperature, and
precipitation — from Open-Meteo **now** (latest analysis hour), forecast
models (GFS, ECMWF IFS), or historical station reports (NOAA CDO daily
summaries and Meteostat hourly dumps). Reports and forecasts stay labeled
as such. History sources carry a time cursor: a day on CDO, an hour on
Meteostat. A local WRF-ARW run is an opt-in forecast producer: the engine
estimates wall time from domain area, grid spacing, forecast hours, and
cores, then an explicit command may start the WRF Docker image on this
machine. The engine rasterizes fields onto the same sweep texture the
shader already samples; values still do not enter QML. Live mode may also
carry an aviation briefing for the view: METAR (observed), TAF as issued
bulletin text, SIGMET / AIRMET / GAMET / issued GRAMET polygons, and a
route GRAMET from origin and destination ICAO plus cruise TAS. That
briefing is bulletin text and a polyline, not a drawn model field. Raw
GRIB / NetCDF files stay out of the UI; WRF wrfout stays in the cache
directory.

The live poller follows the latest volume. `try_next` returning no chunk is
normal between chunks, but 90 seconds with no chunk at all means the
iterator is stuck on a volume that will never grow; restart discovery.
Independently, if the poller task has exited, or the newest radial is thirty
minutes old and discovery has not been tried since, spawn a new poller.
Reselecting the current station is a no-op while the poller is running; if
the task has ended, start it again. Cached frames stay on screen through a
rediscovery. A rediscovery that finds only a sweep already in the catalog
leaves the frame and connection chrome alone; a newer volume still clears
UNAVAILABLE / OFFLINE.

An optional current-conditions source (WeeWX, WeatherAPI, OpenWeatherMap,
Tomorrow.io, Visual Crossing) is the user's choice: they pick the source and
enter their API key, or a WeeWX JSON URL. The engine fetches the current
observation only — no forecasts — and the chrome shows temperature, condition,
source, and observation time. The key lives in `weather.toml` (or `[weather]`
in config.toml), travels to the engine over the local socket, and never enters
`state.json` or the `state` broadcast. Unset, nothing is fetched.

## Scope

Keep the feature set small. Prefer the weather panel, the theme, and the
engine's state over a parallel mechanism in this app. If a visual call is
open, change the running picture and look at it.

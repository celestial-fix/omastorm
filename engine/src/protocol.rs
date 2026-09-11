//! Wire types for `docs/protocol.md`, version 1. Objects serialize in
//! declaration order; clients read keys by name, so order is not significant.
#![allow(
    dead_code,
    reason = "wire fields are written by Serialize for the UI and never read here"
)]

use serde::{Deserialize, Serialize};

pub const VERSION: u32 = 1;

/// Engine to client. Borrows so a snapshot never clones the state tree.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message<'a> {
    Hello(&'a Hello),
    State(&'a State),
    Error(&'a Rejection<'a>),
    TileReady(&'a TileReady<'a>),
    Places(&'a Places<'a>),
    ReportReady(&'a ReportReady<'a>),
}

/// One tile answering a client's `tiles_needed`, sent to that client alone
/// (docs/protocol.md, basemap tiles). Like `Error` it is a reply, not shared
/// state: a tile already rendered this session is answered at once.
#[derive(Serialize)]
pub struct TileReady<'a> {
    pub v: u32,
    /// `ne` (Natural Earth, embedded) or `osm` (fetched vector tile).
    pub set: &'a str,
    pub z: u32,
    pub x: u32,
    pub y: u32,
    /// `tiles/<set>/<z>/<x>/<file>`, relative to the runtime directory.
    pub path: &'a str,
    /// The tile's places for the overlay.
    pub labels: Vec<Label>,
}

/// A place label carried by `tile_ready`; `class` and `rank` follow the
/// OpenMapTiles `place` vocabulary (lower rank is more important).
/// `region` and `country` come from Natural Earth (`adm1name`, `iso_a2`)
/// so the location picker can tell two Jacksonvilles apart; empty on OSM
/// labels and omitted on the wire when empty.
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug, Default)]
pub struct Label {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub class: String,
    pub rank: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub region: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub country: String,
}

/// A finished chart answering one client's `export_report`. A reply, not
/// shared state: only the sender hears it, and `state` does not change.
#[derive(Serialize)]
pub struct ReportReady<'a> {
    pub v: u32,
    /// `reports/<file>`, relative to `$XDG_DATA_HOME/omastorm/`.
    pub path: &'a str,
    pub projection: &'a str,
    pub layers: &'a [String],
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
    pub width: u32,
    pub height: u32,
}

/// Places answering one client's `search_places`. A reply, not shared state:
/// only the sender hears it, and `state` does not change.
#[derive(Serialize)]
pub struct Places<'a> {
    pub v: u32,
    pub query: &'a str,
    pub results: &'a [Label],
}

/// The engine's answer to one client's command it could not carry out. Sent
/// only to that client; `state` is untouched and not broadcast, so nothing a
/// client sends can erase a condition another client is showing.
#[derive(Serialize)]
pub struct Rejection<'a> {
    pub v: u32,
    /// The command's `type`, as the client wrote it.
    pub command: &'a str,
    pub message: &'a str,
}

#[derive(Serialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Hello {
    pub v: u32,
    pub engine: &'static str,
    pub pid: u32,
    pub build: String,
    pub sites: Vec<Station>,
    pub sites_source: String,
    pub sites_retrieved: String,
    pub sites_notes: String,
}

/// The launcher's view of a running daemon's hello. Only the fields needed to
/// recognize a matching build, so a daemon of another build still names its PID.
#[derive(Deserialize)]
pub struct Handshake {
    #[serde(rename = "type")]
    pub kind: String,
    pub v: u32,
    pub pid: u32,
    pub build: String,
}

/// `engine/data/sites.json`.
#[derive(Deserialize)]
pub struct SiteTable {
    pub source: String,
    pub retrieved: String,
    pub notes: String,
    pub sites: Vec<Station>,
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Station {
    pub id: String,
    pub name: String,
    pub state: String,
    pub lat: f64,
    pub lon: f64,
    pub alt_m: f64,
}

#[derive(Serialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub v: u32,
    pub source: Source,
    pub connection: Connection,
    pub site: SiteSelection,
    pub timeline: Vec<TimelineEntry>,
    pub frame: Frame,
    /// The tile sources (`docs/protocol.md`, `tile_ready`).
    pub basemap: Basemap,
    /// Aviation briefing for the last settled view centre (`docs/protocol.md`).
    pub aviation: Aviation,
    /// Products and altitudes the engine can draw (`docs/protocol.md`).
    pub layers: Layers,
    /// Local WRF-ARW producer (`docs/protocol.md`). Idle until estimated or run.
    pub wrf: Wrf,
    /// NOAA CDO / Meteostat historical reports (`docs/protocol.md`).
    pub history: History,
    pub playing: bool,
    /// Current conditions from the user's weather source, omitted when unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weather: Option<Weather>,
}

/// `state.weather`: one current observation. Never carries the API key.
#[derive(Serialize, PartialEq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Weather {
    pub source: String,
    pub source_name: String,
    pub status: WeatherStatus,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub observed_at: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature_c: Option<f64>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub condition: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wind_kmh: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub humidity: Option<f64>,
    pub attribution: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub message: String,
}

#[derive(Serialize, PartialEq, Clone, Copy, Debug)]
#[serde(rename_all = "lowercase")]
pub enum WeatherStatus {
    Ok,
    Loading,
    Stale,
    Unavailable,
    Offline,
}

/// `state.basemap`: what draws the tiles and, for `osm`, whether it can.
#[derive(Serialize, PartialEq, Debug)]
pub struct Basemap {
    pub ne: NaturalEarth,
    pub osm: Osm,
}

#[derive(Serialize, PartialEq, Debug)]
pub struct NaturalEarth {
    pub version: &'static str,
}

/// The `osm` tile source. `status` is the lasting condition of the fetch path,
/// which no client command can clear; `attribution` is shown verbatim by the
/// UI whenever an `osm` tile is on screen.
#[derive(Serialize, PartialEq, Clone, Debug)]
pub struct Osm {
    pub status: OsmStatus,
    pub source: String,
    /// The data version, empty until one is known.
    pub version: String,
    pub attribution: String,
}

/// Observed and advisory aviation products for the view (`docs/protocol.md`).
#[derive(Serialize, PartialEq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Aviation {
    pub status: AviationStatus,
    pub attribution: String,
    pub station: Option<AviationStation>,
    pub metar: Option<Bulletin>,
    pub taf: Option<Bulletin>,
    pub hazards: Vec<Hazard>,
    /// Route GRAMET from `set_gramet` (origin, destination, cruise TAS).
    pub gramet: Gramet,
}

#[derive(Serialize, PartialEq, Clone, Copy, Debug)]
#[serde(rename_all = "lowercase")]
pub enum AviationStatus {
    /// Live, but no view centre has been briefed yet; archived stays here.
    Idle,
    Loading,
    Ok,
    Offline,
    Unavailable,
}

#[derive(Serialize, PartialEq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AviationStation {
    pub id: String,
    pub lat: f64,
    pub lon: f64,
    pub distance_km: f64,
}

/// One METAR or TAF bulletin. `time` is the observation or issue instant.
#[derive(Serialize, PartialEq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Bulletin {
    pub raw: String,
    pub time: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub valid_from: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub valid_to: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub category: String,
}

#[derive(Serialize, PartialEq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Hazard {
    /// `sigmet`, `airmet`, `gamet`, or `gramet`.
    pub kind: String,
    pub hazard: String,
    pub raw: String,
    pub coords: Vec<Point>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub valid_from: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub valid_to: String,
}

#[derive(Serialize, PartialEq, Clone, Copy, Debug)]
pub struct Point {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Serialize, PartialEq, Clone, Copy, Debug)]
#[serde(rename_all = "lowercase")]
pub enum OsmStatus {
    /// Fetching works, or has not been tried since a version was learnt.
    Ok,
    /// Fetching fails; cached tiles still serve.
    Offline,
    /// No data version is known and nothing is cached.
    Unavailable,
}

/// Whether `path` names a texture file the protocol allows: the literal `tex/`
/// prefix and exactly one further segment that is not empty, `.`, or `..` and
/// holds no `/`, backslash, or NUL (`docs/protocol.md`). `ui/Engine.qml`
/// applies the same rule, so a path that passes here is one the UI will load.
pub fn is_texture_path(path: &str) -> bool {
    match path.strip_prefix("tex/") {
        Some(name) => {
            !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', '\0'])
        }
        None => false,
    }
}

/// Whether `path` names a tile file the protocol allows: the literal `tiles/`
/// prefix, a set (`ne` or `osm`), a zoom and a column as decimal integers,
/// and one further segment under the texture rule (`docs/protocol.md`).
pub fn is_tile_path(path: &str) -> bool {
    let mut parts = path.split('/');
    let decimal = |part: Option<&str>| {
        part.is_some_and(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
    };
    parts.next() == Some("tiles")
        && matches!(parts.next(), Some("ne" | "osm"))
        && decimal(parts.next())
        && decimal(parts.next())
        && parts.next().is_some_and(|name| {
            !name.is_empty() && name != "." && name != ".." && !name.contains(['\\', '\0'])
        })
        && parts.next().is_none()
}

/// Whether `path` names a chart the protocol allows: the literal `reports/`
/// prefix and one further segment under the texture-name rule.
pub fn is_report_path(path: &str) -> bool {
    match path.strip_prefix("reports/") {
        Some(name) => {
            !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', '\0'])
        }
        None => false,
    }
}

impl State {
    /// Every file under `$XDG_RUNTIME_DIR/omastorm/` a client may currently be
    /// reading. Texture cleanup retires a file 30 s after it leaves this set,
    /// so any new path field (azimuth tables, timeline frames) is added here.
    pub fn referenced_files(&self) -> impl Iterator<Item = &str> {
        [self.frame.texture.as_str(), self.frame.azimuth_lut.as_str()]
            .into_iter()
            .filter(|path| !path.is_empty())
    }
}

#[derive(Serialize, PartialEq, Clone, Copy, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Archived,
    Live,
}

#[derive(Serialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub status: ConnectionStatus,
    /// Age of the newest complete frame; 0 while there is none.
    pub age_seconds: u64,
}

/// The lasting condition of the engine's data path (`docs/protocol.md`). In
/// live mode `Ok`, `Stale`, and `Unavailable` follow the age of the newest
/// radial received at each snapshot, so they only ever say how quiet a
/// reachable feed has been; `Loading` is set by a site switch and `Offline`
/// by the poller when the bucket cannot be reached, and the next live sweep
/// clears both.
#[derive(Serialize, PartialEq, Clone, Copy, Debug)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionStatus {
    Ok,
    Stale,
    Unavailable,
    Offline,
    Loading,
}

#[derive(Serialize, PartialEq, Debug)]
pub struct SiteSelection {
    pub id: String,
    pub follow: bool,
    pub locked: bool,
}

/// One frame a client can `seek` to (`docs/protocol.md`, `state.timeline`):
/// the station's complete frames oldest first, then the sweep in progress
/// as a `partial` entry while one is painting.
#[derive(Serialize, PartialEq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TimelineEntry {
    pub id: String,
    pub scan_time: String,
    pub status: FrameStatus,
}

/// `engine/data/fixture.json` plus what the engine decodes and publishes: the
/// polar sweep texture, its azimuth lookup, and the gate geometry the shader
/// needs to place every gate (`docs/protocol.md`, texture files).
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Frame {
    pub id: String,
    pub product: String,
    /// Display name for `product`; the engine owns product vocabulary.
    pub product_name: String,
    pub units: String,
    pub elevation_deg: f64,
    pub scan_time: String,
    pub sweep_end: String,
    pub status: FrameStatus,
    /// Paths relative to `$XDG_RUNTIME_DIR/omastorm/` and the sweep geometry
    /// are decoded at startup, so the fixture file carries none of them.
    #[serde(default)]
    pub texture: String,
    #[serde(default)]
    pub azimuth_lut: String,
    #[serde(default)]
    pub rays: u32,
    #[serde(default)]
    pub gates: u32,
    #[serde(default)]
    pub first_gate_m: u32,
    #[serde(default)]
    pub gate_spacing_m: u32,
    /// Measured value = (code - offset) / scale, the moment's own encoding;
    /// the UI turns a dBZ floor into a code threshold with them. Zero while
    /// nothing is decoded (the loading placeholder), which disables the floor.
    #[serde(default)]
    pub scale: f32,
    #[serde(default)]
    pub offset: f32,
    pub site: Geometry,
    pub palette: Vec<String>,
    pub bounds: Vec<i32>,
    /// `sweep` (Level II) or `field` (wind / pressure / water). Empty is sweep.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub altitude_hpa: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub altitude_name: String,
    /// `nexrad` (empty on Level II fixtures), `gfs`, or `ecmwf`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub layer_source: String,
}

fn is_zero_u32(value: &u32) -> bool {
    *value == 0
}

/// The products, altitudes, and sources `set_product` / `set_source` accept.
#[derive(Serialize, PartialEq, Clone, Debug)]
pub struct Layers {
    pub attribution: String,
    pub sources: Vec<LayerSource>,
    pub products: Vec<LayerProduct>,
}

#[derive(Serialize, PartialEq, Clone, Debug)]
pub struct LayerSource {
    pub id: String,
    pub name: String,
    pub kind: String,
    /// `report` (observations / now) or `forecast` (NWP, including local WRF).
    pub group: String,
    pub model: String,
}

/// Local WRF run status and the last wall-clock estimate.
#[derive(Serialize, PartialEq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Wrf {
    pub status: WrfStatus,
    pub image: String,
    pub lat: f64,
    pub lon: f64,
    pub message: String,
    pub estimate: WrfEstimate,
}

#[derive(Serialize, PartialEq, Clone, Copy, Debug)]
#[serde(rename_all = "snake_case")]
pub enum WrfStatus {
    Idle,
    MissingDocker,
    Queued,
    Running,
    Ok,
    Failed,
}

#[derive(Serialize, PartialEq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WrfEstimate {
    pub width_km: u32,
    pub height_km: u32,
    pub area_km2: u32,
    pub dx_km: f64,
    pub hours: u32,
    pub cores: u32,
    pub levels: u32,
    pub nx: u32,
    pub ny: u32,
    pub cells: u32,
    pub dt_sec: f64,
    pub steps: u32,
    pub grib_files: u32,
    pub download_min: u32,
    pub preprocess_min: u32,
    pub integrate_min: u32,
    pub total_min: u32,
    pub total_min_low: u32,
    pub total_min_high: u32,
    pub memory_mb: u32,
    pub summary: String,
}

/// Route GRAMET: origin and destination ICAO, cruise TAS, and the sampled track.
#[derive(Serialize, PartialEq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Gramet {
    pub status: AviationStatus,
    pub origin: Option<GrametFix>,
    pub destination: Option<GrametFix>,
    pub cruise_kt: u32,
    pub flight_level: u32,
    pub distance_km: f64,
    pub ete_min: u32,
    pub raw: String,
    pub coords: Vec<Point>,
    pub legs: Vec<GrametLeg>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub message: String,
}

#[derive(Serialize, PartialEq, Clone, Debug)]
pub struct GrametFix {
    pub icao: String,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Serialize, PartialEq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GrametLeg {
    pub dist_km: f64,
    pub ete_min: u32,
    pub lat: f64,
    pub lon: f64,
    pub wind_kt: f64,
    pub wind_dir: f64,
    pub temp_c: f64,
    pub rh: f64,
}

/// Historical station reports (NOAA CDO / Meteostat) with a time cursor.
#[derive(Serialize, PartialEq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct History {
    pub status: AviationStatus,
    pub source: String,
    pub time: String,
    pub step: String,
    pub attribution: String,
    pub stations: Vec<HistoryStation>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub message: String,
}

#[derive(Serialize, PartialEq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct HistoryStation {
    pub id: String,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub distance_km: f64,
}

#[derive(Serialize, PartialEq, Clone, Debug)]
pub struct LayerProduct {
    pub code: String,
    pub name: String,
    pub units: String,
    pub kind: String,
    pub altitudes: Vec<LayerAltitude>,
}

#[derive(Serialize, PartialEq, Clone, Debug)]
pub struct LayerAltitude {
    pub index: u32,
    pub name: String,
    pub hpa: u32,
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Copy, Debug)]
#[serde(rename_all = "lowercase")]
pub enum FrameStatus {
    Complete,
    Partial,
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Geometry {
    pub lat: f64,
    pub lon: f64,
    pub alt_m: f64,
}

/// Client to engine. A known type with missing or mistyped fields fails to
/// deserialize; the engine answers the sender with a `Rejection`.
#[derive(Deserialize, PartialEq, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    /// Go live on a station from the `hello` table: the newest
    /// cached frame or an empty one shows at once, and the poller follows.
    SelectSite {
        id: String,
    },
    Follow {
        enabled: bool,
    },
    Lock {
        enabled: bool,
    },
    Play,
    Pause,
    Step {
        delta: i64,
    },
    Seek {
        id: String,
    },
    #[serde(rename_all = "camelCase")]
    SetProduct {
        product: String,
        elevation_index: u32,
    },
    SetSource {
        source: String,
    },
    /// Build a route GRAMET from origin and destination ICAO and cruise TAS.
    #[serde(rename_all = "camelCase")]
    SetGramet {
        origin: String,
        destination: String,
        cruise_kt: u32,
        #[serde(default)]
        flight_level: u32,
    },
    /// Jump the CDO / Meteostat cursor to an ISO date or hour.
    SeekHistory {
        time: String,
    },
    /// Step the CDO / Meteostat cursor by whole days or hours.
    StepHistory {
        delta: i64,
    },
    /// Recompute the WRF wall-clock estimate for the view (`docs/protocol.md`).
    #[serde(rename_all = "camelCase")]
    EstimateWrf {
        lat: f64,
        lon: f64,
        #[serde(default)]
        width_km: f64,
        #[serde(default)]
        height_km: f64,
        #[serde(default)]
        dx_km: f64,
        #[serde(default)]
        hours: u32,
        #[serde(default)]
        cores: u32,
    },
    /// Start a local WRF Docker run for the last estimate (live only).
    #[serde(rename_all = "camelCase")]
    RunWrf {
        lat: f64,
        lon: f64,
        #[serde(default)]
        width_km: f64,
        #[serde(default)]
        height_km: f64,
        #[serde(default)]
        dx_km: f64,
        #[serde(default)]
        hours: u32,
        #[serde(default)]
        cores: u32,
    },
    /// The visible inclusive tile rectangle at one zoom, at most 64 tiles
    /// Answered tile by tile with `tile_ready` to the sender.
    TilesNeeded {
        z: u32,
        x0: u32,
        y0: u32,
        x1: u32,
        y1: u32,
    },
    /// The map centre when a pan settles: with `follow` on and
    /// `lock` off the engine hands off to the nearest station.
    ViewCenter {
        lat: f64,
        lon: f64,
    },
    /// Rank gazetteer places for the location picker (worldwide GeoNames
    /// ≥ 5000). Answered with `places` to the sender; optional `lat`/`lon`
    /// order nearer matches first.
    SearchPlaces {
        query: String,
        #[serde(default)]
        lat: Option<f64>,
        #[serde(default)]
        lon: Option<f64>,
    },
    /// Choose or clear the current-conditions source. `apiKey` never appears
    /// in `state`. An empty `source` turns the feed off.
    #[serde(rename_all = "camelCase")]
    SetWeather {
        #[serde(default)]
        source: String,
        #[serde(default)]
        api_key: String,
        #[serde(default)]
        url: String,
        #[serde(default)]
        lat: Option<f64>,
        #[serde(default)]
        lon: Option<f64>,
    },
    /// Raster a Lambert conformal conic chart of the box. Answered with
    /// `report_ready` to the sender; `layers` names the overlays to draw.
    ExportReport {
        west: f64,
        south: f64,
        east: f64,
        north: f64,
        layers: Vec<String>,
        #[serde(default)]
        width: Option<u32>,
    },
    /// Anything newer than this build.
    #[serde(other)]
    Unsupported,
}

#[cfg(test)]
mod tests {
    use super::{is_report_path, is_texture_path, is_tile_path};

    #[test]
    fn report_paths_are_one_segment_under_reports() {
        assert!(is_report_path("reports/KTLX-20130520T201643Z-lcc-r1.png"));
        for bad in [
            "",
            "reports",
            "reports/",
            "reports/.",
            "reports/a/b.png",
            "tex/a.png",
        ] {
            assert!(!is_report_path(bad), "{bad:?}");
        }
    }

    #[test]
    fn tile_paths_are_set_zoom_column_and_one_file() {
        for ok in [
            "tiles/ne/5/7/12-3f9a1c2e.png",
            "tiles/osm/11/470/808-3f9a1c2e.png",
            "tiles/ne/0/0/0-00000000.png",
            "tiles/ne/1/0/a",
        ] {
            assert!(is_tile_path(ok), "{ok} should be accepted");
        }
        for bad in [
            "",
            "tiles",
            "tiles/ne/5/7/",
            "tiles/ne/5/7",
            "tiles/ne/5/7/12-a.png/x",
            "tiles/ne/5/7/.",
            "tiles/ne/5/7/..",
            "tiles/ne/5/../12-a.png",
            "tiles/ne/-1/7/12-a.png",
            "tiles/ne/5/7a/12-a.png",
            "tiles/foo/5/7/12-a.png",
            "tiles/ne/5/7/12\\a.png",
            "tiles/ne/5/7/12\0.png",
            "tex/ne/5/7/12-a.png",
            "/tiles/ne/5/7/12-a.png",
        ] {
            assert!(!is_tile_path(bad), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn texture_paths_are_one_segment_under_tex() {
        for ok in [
            "tex/sweep-KTLX-20130520T201643Z-e0-1-r2.png",
            "tex/sweep-TEST-r1.png",
            "tex/azlut-KTLX-20130520T201643Z-e0-r3.png",
            "tex/...",
            "tex/a",
        ] {
            assert!(is_texture_path(ok), "{ok} should be accepted");
        }
        for bad in [
            "",
            "tex",
            "tex/",
            "tex/.",
            "tex/..",
            "tex/../x.png",
            "tex/a/b.png",
            "tex//a.png",
            "tex/a\\b.png",
            "tex/a\0.png",
            "/tex/a.png",
            "text/a.png",
            "TEX/a.png",
            "../tex/a.png",
            "a.png",
        ] {
            assert!(!is_texture_path(bad), "{bad:?} should be rejected");
        }
    }
}

//! Atmospheric field layers for the view: wind, pressure, and water
//! (relative humidity) at surface and standard pressure altitudes.
//! Live mode fetches the current hour from Open-Meteo for a small grid
//! around the centre and rasterizes a polar sweep the existing shader
//! can draw. Archived daemons and ordinary checks never fetch.
//! `OMASTORM_FIELDS_URL` overrides the API root.

use crate::protocol::{
    Frame, FrameStatus, Geometry, LayerAltitude, LayerProduct, LayerSource, Layers,
};
use crate::sweep;
use serde::Deserialize;
use std::{
    env, io,
    time::{Duration, Instant},
};

const DEFAULT_URL: &str = "https://api.open-meteo.com/v1/forecast";
const USER_AGENT: &str = concat!(
    "omastorm/",
    env!("CARGO_PKG_VERSION"),
    " (https://omastorm.com)"
);
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_BODY: usize = 4 << 20;
const GRID: usize = 7;
const HALF_DEG: f64 = 2.0;
const OUT_GATES: u32 = 64;
const OUT_RAYS: u32 = 360;
const RANGE_M: f64 = 280_000.0;
const STILL_KM: f64 = 25.0;
const FRESH: Duration = Duration::from_secs(10 * 60);
const ATTRIBUTION: &str = "NOAA NEXRAD · Open-Meteo";

const WIND_PALETTE: [&str; 9] = [
    "#1b4d6e", "#2a7ab0", "#4aa36a", "#c9c84a", "#e09a3e", "#d4653a", "#c23b55", "#8e3d8a",
    "#f0d4ee",
];
const WIND_BOUNDS: [i32; 10] = [0, 5, 10, 15, 20, 25, 30, 40, 50, 80];
const WATER_PALETTE: [&str; 8] = [
    "#3d4a3a", "#4f7a52", "#6fa36a", "#8fc47a", "#c9d96b", "#7eb8c9", "#4a88b8", "#2a5080",
];
const WATER_BOUNDS: [i32; 9] = [0, 20, 40, 55, 70, 80, 90, 96, 101];
const TEMP_PALETTE: [&str; 8] = [
    "#2a3a8a", "#3d7ab0", "#4aa39a", "#8fc47a", "#c9c84a", "#e09a3e", "#d4653a", "#8a2a4a",
];
const TEMP_BOUNDS: [i32; 9] = [-30, -10, 0, 8, 16, 22, 28, 34, 45];
const PRECIP_PALETTE: [&str; 8] = [
    "#3d4a3a", "#4f7a52", "#6fa36a", "#8fc47a", "#7eb8c9", "#4a88b8", "#3d4a8a", "#5a2a6e",
];
const PRECIP_BOUNDS: [i32; 9] = [0, 1, 3, 6, 10, 20, 35, 55, 80];
const PRES_PALETTE: [&str; 8] = [
    "#5a2a6e", "#3d4a8a", "#2a7ab0", "#4aa39a", "#c9c84a", "#e09a3e", "#d4653a", "#a33b4a",
];
const PRES_BOUNDS: [i32; 9] = [980, 990, 1000, 1008, 1013, 1018, 1025, 1035, 1055];
const HEIGHT_PALETTE: [&str; 8] = [
    "#2a3a5a", "#3d5a8a", "#4a88b8", "#6fa36a", "#c9c84a", "#e09a3e", "#d4653a", "#a33b4a",
];

#[derive(Clone, Copy)]
pub struct Altitude {
    pub index: u32,
    pub name: &'static str,
    pub hpa: u32,
}

pub const ALTITUDES: [Altitude; 6] = [
    Altitude {
        index: 0,
        name: "SFC",
        hpa: 0,
    },
    Altitude {
        index: 1,
        name: "2 500 ft · 925 hPa",
        hpa: 925,
    },
    Altitude {
        index: 2,
        name: "5 000 ft · 850 hPa",
        hpa: 850,
    },
    Altitude {
        index: 3,
        name: "10 000 ft · 700 hPa",
        hpa: 700,
    },
    Altitude {
        index: 4,
        name: "18 000 ft · 500 hPa",
        hpa: 500,
    },
    Altitude {
        index: 5,
        name: "30 000 ft · 300 hPa",
        hpa: 300,
    },
];

#[derive(Clone, Copy)]
pub struct Source {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: &'static str,
    pub group: &'static str,
    pub model: &'static str,
}

pub const SOURCES: [Source; 10] = [
    Source {
        id: "nexrad",
        name: "NEXRAD",
        kind: "sweep",
        group: "report",
        model: "",
    },
    Source {
        id: "now",
        name: "Open-Meteo now",
        kind: "analysis",
        group: "report",
        model: "best_match",
    },
    Source {
        id: "gfs",
        name: "GFS",
        kind: "model",
        group: "forecast",
        model: "gfs_global",
    },
    Source {
        id: "ecmwf",
        name: "ECMWF IFS",
        kind: "model",
        group: "forecast",
        model: "ecmwf_ifs025",
    },
    Source {
        id: "wrf",
        name: "WRF",
        kind: "local",
        group: "forecast",
        model: "wrf",
    },
    Source {
        id: "dmc",
        name: "MeteoChile",
        kind: "analysis",
        group: "report",
        model: "",
    },
    Source {
        id: "dmc_wrf_gfs",
        name: "WRF-DMC GFS",
        kind: "model",
        group: "forecast",
        model: "wrf_dmc_gfs",
    },
    Source {
        id: "dmc_wrf_ecmwf",
        name: "WRF-DMC ECMWF",
        kind: "model",
        group: "forecast",
        model: "wrf_dmc_ecmwf",
    },
    Source {
        id: "cdo",
        name: "NOAA CDO",
        kind: "archive",
        group: "report",
        model: "",
    },
    Source {
        id: "meteostat",
        name: "Meteostat",
        kind: "archive",
        group: "report",
        model: "",
    },
];

pub fn layers() -> Layers {
    let altitudes = ALTITUDES
        .iter()
        .map(|a| LayerAltitude {
            index: a.index,
            name: a.name.into(),
            hpa: a.hpa,
        })
        .collect::<Vec<_>>();
    Layers {
        attribution: ATTRIBUTION.into(),
        sources: SOURCES
            .iter()
            .map(|s| LayerSource {
                id: s.id.into(),
                name: s.name.into(),
                kind: s.kind.into(),
                group: s.group.into(),
                model: s.model.into(),
            })
            .collect(),
        products: vec![
            LayerProduct {
                code: "REF".into(),
                name: "Reflectivity".into(),
                units: "dBZ".into(),
                kind: "sweep".into(),
                altitudes: vec![LayerAltitude {
                    index: 0,
                    name: "lowest cut".into(),
                    hpa: 0,
                }],
            },
            LayerProduct {
                code: "WIND".into(),
                name: "Wind".into(),
                units: "kt".into(),
                kind: "field".into(),
                altitudes: altitudes.clone(),
            },
            LayerProduct {
                code: "PRES".into(),
                name: "Pressure".into(),
                units: "hPa".into(),
                kind: "field".into(),
                altitudes: altitudes.clone(),
            },
            LayerProduct {
                code: "WATER".into(),
                name: "Water".into(),
                units: "%".into(),
                kind: "field".into(),
                altitudes: altitudes.clone(),
            },
            LayerProduct {
                code: "TEMP".into(),
                name: "Temperature".into(),
                units: "°C".into(),
                kind: "field".into(),
                altitudes: vec![altitudes[0].clone()],
            },
            LayerProduct {
                code: "PRECIP".into(),
                name: "Precipitation".into(),
                units: "mm".into(),
                kind: "field".into(),
                altitudes: vec![altitudes[0].clone()],
            },
        ],
    }
}

pub fn is_field(product: &str) -> bool {
    matches!(product, "WIND" | "PRES" | "WATER" | "TEMP" | "PRECIP")
}

pub fn is_history(id: &str) -> bool {
    matches!(id, "cdo" | "meteostat")
}

pub fn source(id: &str) -> Option<Source> {
    SOURCES.iter().copied().find(|s| s.id == id)
}

pub fn is_model(id: &str) -> bool {
    source(id).is_some_and(|s| s.kind == "model" || s.kind == "analysis")
}

pub fn altitude(index: u32) -> Option<Altitude> {
    ALTITUDES.iter().copied().find(|a| a.index == index)
}

pub struct Client {
    base: String,
    http: reqwest::Client,
    last: std::sync::Mutex<Option<Cache>>,
}

struct Cache {
    lat: f64,
    lon: f64,
    model: String,
    at: Instant,
    samples: Vec<Sample>,
    time: String,
}

#[derive(Clone, Default)]
pub(crate) struct Sample {
    pub lat: f64,
    pub lon: f64,
    pub wind_kt: [Option<f64>; 6],
    pub wind_dir: [Option<f64>; 6],
    pub pressure_hpa: Option<f64>,
    pub height_m: [Option<f64>; 6],
    pub water_pct: [Option<f64>; 6],
    pub temp_c: [Option<f64>; 6],
    pub precip_mm: Option<f64>,
}

impl Client {
    pub fn open() -> io::Result<Client> {
        let base = env::var("OMASTORM_FIELDS_URL").unwrap_or_else(|_| DEFAULT_URL.into());
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(FETCH_TIMEOUT)
            .build()
            .map_err(io::Error::other)?;
        Ok(Client {
            base,
            http,
            last: std::sync::Mutex::new(None),
        })
    }

    pub fn should_refresh(&self, lat: f64, lon: f64, model: &str) -> bool {
        match &*self.last.lock().unwrap() {
            None => true,
            Some(last) => {
                last.model != model
                    || last.at.elapsed() >= FRESH
                    || great_circle_km(last.lat, last.lon, lat, lon) >= STILL_KM
            }
        }
    }

    pub async fn fetch(
        &self,
        lat: f64,
        lon: f64,
        source_id: &str,
        product: &str,
        elevation_index: u32,
    ) -> io::Result<(Frame, Vec<u8>, Vec<u8>)> {
        let source = source(source_id)
            .filter(|s| s.kind == "model" || s.kind == "analysis")
            .ok_or_else(|| io::Error::other(format!("unknown field source {source_id}")))?;
        let altitude = altitude(elevation_index)
            .ok_or_else(|| io::Error::other(format!("unknown field altitude {elevation_index}")))?;
        if self.should_refresh(lat, lon, source.model) {
            let (samples, time) = self.load(lat, lon, source.model).await?;
            *self.last.lock().unwrap() = Some(Cache {
                lat,
                lon,
                model: source.model.into(),
                at: Instant::now(),
                samples,
                time,
            });
        }
        let cache = self.last.lock().unwrap();
        let cache = cache
            .as_ref()
            .ok_or_else(|| io::Error::other("field cache is empty"))?;
        raster(
            &cache.samples,
            lat,
            lon,
            source.id,
            product,
            altitude,
            &cache.time,
        )
    }
}

impl Client {
    async fn load(&self, lat: f64, lon: f64, model: &str) -> io::Result<(Vec<Sample>, String)> {
        let mut lats = Vec::new();
        let mut lons = Vec::new();
        for iy in 0..GRID {
            for ix in 0..GRID {
                let t = if GRID == 1 {
                    0.0
                } else {
                    iy as f64 / (GRID - 1) as f64
                };
                let u = if GRID == 1 {
                    0.0
                } else {
                    ix as f64 / (GRID - 1) as f64
                };
                lats.push(lat + HALF_DEG - t * 2.0 * HALF_DEG);
                lons.push(lon - HALF_DEG + u * 2.0 * HALF_DEG);
            }
        }
        let hourly = [
            "wind_speed_10m",
            "wind_direction_10m",
            "pressure_msl",
            "relative_humidity_2m",
            "temperature_2m",
            "precipitation",
            "wind_speed_925hPa",
            "wind_direction_925hPa",
            "relative_humidity_925hPa",
            "geopotential_height_925hPa",
            "wind_speed_850hPa",
            "wind_direction_850hPa",
            "relative_humidity_850hPa",
            "geopotential_height_850hPa",
            "wind_speed_700hPa",
            "wind_direction_700hPa",
            "relative_humidity_700hPa",
            "geopotential_height_700hPa",
            "wind_speed_500hPa",
            "wind_direction_500hPa",
            "relative_humidity_500hPa",
            "geopotential_height_500hPa",
            "wind_speed_300hPa",
            "wind_direction_300hPa",
            "relative_humidity_300hPa",
            "geopotential_height_300hPa",
        ]
        .join(",");
        let url = format!(
            "{}?latitude={}&longitude={}&hourly={}&wind_speed_unit=kn&forecast_days=1&timezone=UTC&models={}",
            self.base,
            lats.iter()
                .map(|v| format!("{v:.4}"))
                .collect::<Vec<_>>()
                .join(","),
            lons.iter()
                .map(|v| format!("{v:.4}"))
                .collect::<Vec<_>>()
                .join(","),
            hourly,
            model
        );
        let response = self.http.get(&url).send().await.map_err(io::Error::other)?;
        if !response.status().is_success() {
            return Err(io::Error::other(format!(
                "{url} returned {}",
                response.status()
            )));
        }
        let bytes = response.bytes().await.map_err(io::Error::other)?;
        if bytes.len() > MAX_BODY {
            return Err(io::Error::other("field body too large"));
        }
        parse_grid(&bytes, &lats, &lons)
    }
}

#[derive(Deserialize)]
struct Place {
    #[serde(default)]
    hourly: Hourly,
}

#[derive(Deserialize, Default)]
struct Hourly {
    #[serde(default)]
    time: Vec<String>,
    #[serde(default)]
    wind_speed_10m: Vec<Option<f64>>,
    #[serde(default)]
    wind_direction_10m: Vec<Option<f64>>,
    #[serde(default)]
    pressure_msl: Vec<Option<f64>>,
    #[serde(default)]
    relative_humidity_2m: Vec<Option<f64>>,
    #[serde(default)]
    temperature_2m: Vec<Option<f64>>,
    #[serde(default)]
    precipitation: Vec<Option<f64>>,
    #[serde(default, rename = "wind_speed_925hPa")]
    wind_speed_925: Vec<Option<f64>>,
    #[serde(default, rename = "wind_direction_925hPa")]
    wind_direction_925: Vec<Option<f64>>,
    #[serde(default, rename = "relative_humidity_925hPa")]
    humidity_925: Vec<Option<f64>>,
    #[serde(default, rename = "geopotential_height_925hPa")]
    height_925: Vec<Option<f64>>,
    #[serde(default, rename = "wind_speed_850hPa")]
    wind_speed_850: Vec<Option<f64>>,
    #[serde(default, rename = "wind_direction_850hPa")]
    wind_direction_850: Vec<Option<f64>>,
    #[serde(default, rename = "relative_humidity_850hPa")]
    humidity_850: Vec<Option<f64>>,
    #[serde(default, rename = "geopotential_height_850hPa")]
    height_850: Vec<Option<f64>>,
    #[serde(default, rename = "wind_speed_700hPa")]
    wind_speed_700: Vec<Option<f64>>,
    #[serde(default, rename = "wind_direction_700hPa")]
    wind_direction_700: Vec<Option<f64>>,
    #[serde(default, rename = "relative_humidity_700hPa")]
    humidity_700: Vec<Option<f64>>,
    #[serde(default, rename = "geopotential_height_700hPa")]
    height_700: Vec<Option<f64>>,
    #[serde(default, rename = "wind_speed_500hPa")]
    wind_speed_500: Vec<Option<f64>>,
    #[serde(default, rename = "wind_direction_500hPa")]
    wind_direction_500: Vec<Option<f64>>,
    #[serde(default, rename = "relative_humidity_500hPa")]
    humidity_500: Vec<Option<f64>>,
    #[serde(default, rename = "geopotential_height_500hPa")]
    height_500: Vec<Option<f64>>,
    #[serde(default, rename = "wind_speed_300hPa")]
    wind_speed_300: Vec<Option<f64>>,
    #[serde(default, rename = "wind_direction_300hPa")]
    wind_direction_300: Vec<Option<f64>>,
    #[serde(default, rename = "relative_humidity_300hPa")]
    humidity_300: Vec<Option<f64>>,
    #[serde(default, rename = "geopotential_height_300hPa")]
    height_300: Vec<Option<f64>>,
}

fn parse_grid(bytes: &[u8], lats: &[f64], lons: &[f64]) -> io::Result<(Vec<Sample>, String)> {
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(io::Error::other)?;
    let places: Vec<Place> = if value.is_array() {
        serde_json::from_value(value).map_err(io::Error::other)?
    } else {
        vec![serde_json::from_value(value).map_err(io::Error::other)?]
    };
    if places.is_empty() {
        return Err(io::Error::other("field response is empty"));
    }
    let hour = current_hour(&places[0].hourly.time);
    let time = places[0].hourly.time.get(hour).cloned().unwrap_or_default();
    let mut samples = Vec::new();
    for (i, place) in places.iter().enumerate() {
        let hourly = &place.hourly;
        samples.push(Sample {
            lat: *lats.get(i).unwrap_or(&0.0),
            lon: *lons.get(i).unwrap_or(&0.0),
            wind_kt: [
                at(&hourly.wind_speed_10m, hour),
                at(&hourly.wind_speed_925, hour),
                at(&hourly.wind_speed_850, hour),
                at(&hourly.wind_speed_700, hour),
                at(&hourly.wind_speed_500, hour),
                at(&hourly.wind_speed_300, hour),
            ],
            wind_dir: [
                at(&hourly.wind_direction_10m, hour),
                at(&hourly.wind_direction_925, hour),
                at(&hourly.wind_direction_850, hour),
                at(&hourly.wind_direction_700, hour),
                at(&hourly.wind_direction_500, hour),
                at(&hourly.wind_direction_300, hour),
            ],
            pressure_hpa: at(&hourly.pressure_msl, hour),
            height_m: [
                None,
                at(&hourly.height_925, hour),
                at(&hourly.height_850, hour),
                at(&hourly.height_700, hour),
                at(&hourly.height_500, hour),
                at(&hourly.height_300, hour),
            ],
            water_pct: [
                at(&hourly.relative_humidity_2m, hour),
                at(&hourly.humidity_925, hour),
                at(&hourly.humidity_850, hour),
                at(&hourly.humidity_700, hour),
                at(&hourly.humidity_500, hour),
                at(&hourly.humidity_300, hour),
            ],
            temp_c: [
                at(&hourly.temperature_2m, hour),
                None,
                None,
                None,
                None,
                None,
            ],
            precip_mm: at(&hourly.precipitation, hour),
        });
    }
    Ok((samples, iso_hour(&time)))
}

fn current_hour(times: &[String]) -> usize {
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:00").to_string();
    times
        .iter()
        .rposition(|t| t.replace('Z', "").starts_with(&now) || t.as_str() <= now.as_str())
        .unwrap_or(0)
}

fn at(values: &[Option<f64>], hour: usize) -> Option<f64> {
    values.get(hour).copied().flatten()
}

fn iso_hour(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.ends_with('Z') {
        return trimmed.to_owned();
    }
    if trimmed.len() == 16 {
        return format!("{trimmed}:00Z");
    }
    format!("{trimmed}Z")
}

pub(crate) fn raster(
    samples: &[Sample],
    lat: f64,
    lon: f64,
    source_id: &str,
    product: &str,
    altitude: Altitude,
    time: &str,
) -> io::Result<(Frame, Vec<u8>, Vec<u8>)> {
    let (name, units, palette, bounds) = vocabulary(product, altitude);
    let level = altitude.index as usize;
    let spacing = (RANGE_M / f64::from(OUT_GATES)).round() as u32;
    let mut pixels = Vec::with_capacity(OUT_RAYS as usize * OUT_GATES as usize * 4);
    for ray in 0..OUT_RAYS {
        let az = f64::from(ray) + 0.5;
        for gate in 0..OUT_GATES {
            let range = (f64::from(gate) + 0.5) * f64::from(spacing);
            let (plat, plon) = destination(lat, lon, az, range);
            let (value, dir) = interpolate(samples, plat, plon, product, level);
            let (class, code, status) = match value {
                None => (0, 0, 0),
                Some(v) => {
                    let code = v.round().clamp(0.0, 255.0) as u8;
                    let class = class_of(v, bounds) + 1;
                    let status = dir
                        .map(|d| 1 + ((d.rem_euclid(360.0) / 360.0) * 254.0).floor() as u8)
                        .unwrap_or(0);
                    (class, code, status)
                }
            };
            pixels.extend_from_slice(&[class, status, code, 255]);
        }
    }
    let mut lut = Vec::with_capacity(3600 * 4);
    for i in 0..3600 {
        let az = (f64::from(i) + 0.5) / 10.0;
        let row = az.floor() as u16 % OUT_RAYS as u16;
        lut.extend_from_slice(&[(row & 0xff) as u8, (row >> 8) as u8, 0, 255]);
    }
    let texture = sweep::png(OUT_GATES, OUT_RAYS, &pixels)?;
    let azimuth_lut = sweep::png(3600, 1, &lut)?;
    let frame = Frame {
        id: format!("{product}-{}-a{}", compact_hour(time), altitude.hpa),
        product: product.into(),
        product_name: name.into(),
        units: units.into(),
        elevation_deg: 0.0,
        scan_time: time.to_owned(),
        sweep_end: time.to_owned(),
        status: FrameStatus::Complete,
        texture: String::new(),
        azimuth_lut: String::new(),
        rays: OUT_RAYS,
        gates: OUT_GATES,
        first_gate_m: 0,
        gate_spacing_m: spacing.max(1),
        scale: 0.0,
        offset: 0.0,
        site: Geometry {
            lat,
            lon,
            alt_m: 0.0,
        },
        palette: palette.iter().map(|c| (*c).to_owned()).collect(),
        bounds: bounds.to_vec(),
        kind: "field".into(),
        altitude_hpa: altitude.hpa,
        altitude_name: altitude.name.into(),
        layer_source: source_id.into(),
    };
    Ok((frame, texture, azimuth_lut))
}

fn vocabulary(
    product: &str,
    altitude: Altitude,
) -> (
    &'static str,
    &'static str,
    &'static [&'static str],
    &'static [i32],
) {
    match product {
        "WIND" => ("Wind", "kt", &WIND_PALETTE, &WIND_BOUNDS),
        "WATER" => ("Water", "%", &WATER_PALETTE, &WATER_BOUNDS),
        "TEMP" => ("Temperature", "°C", &TEMP_PALETTE, &TEMP_BOUNDS),
        "PRECIP" => ("Precipitation", "mm", &PRECIP_PALETTE, &PRECIP_BOUNDS),
        "PRES" if altitude.hpa == 0 => ("Pressure", "hPa", &PRES_PALETTE, &PRES_BOUNDS),
        _ => (
            "Pressure height",
            "m",
            &HEIGHT_PALETTE,
            &[700, 1200, 1500, 2500, 3000, 5000, 5800, 9000, 10000],
        ),
    }
}

fn class_of(value: f64, bounds: &[i32]) -> u8 {
    if bounds.len() < 2 {
        return 0;
    }
    let classes = bounds.len() - 1;
    let above = bounds.partition_point(|&b| b as f64 <= value);
    above.saturating_sub(1).min(classes - 1) as u8
}

fn interpolate(
    samples: &[Sample],
    lat: f64,
    lon: f64,
    product: &str,
    level: usize,
) -> (Option<f64>, Option<f64>) {
    let mut weight = 0.0;
    let mut sum = 0.0;
    let mut dir_x = 0.0;
    let mut dir_y = 0.0;
    let mut dir_w = 0.0;
    for sample in samples {
        let d = great_circle_km(lat, lon, sample.lat, sample.lon).max(0.01);
        let w = 1.0 / (d * d);
        let value = match product {
            "WIND" => sample.wind_kt.get(level).copied().flatten(),
            "WATER" => sample.water_pct.get(level).copied().flatten(),
            "TEMP" => sample.temp_c.get(level).copied().flatten(),
            "PRECIP" => sample.precip_mm,
            "PRES" if level == 0 => sample.pressure_hpa,
            "PRES" => sample.height_m.get(level).copied().flatten(),
            _ => None,
        };
        if let Some(v) = value {
            weight += w;
            sum += v * w;
        }
        if product == "WIND"
            && let Some(dir) = sample.wind_dir.get(level).copied().flatten()
        {
            let r = dir.to_radians();
            dir_x += r.sin() * w;
            dir_y += r.cos() * w;
            dir_w += w;
        }
    }
    let value = (weight > 0.0).then_some(sum / weight);
    let dir = (dir_w > 0.0).then_some(dir_x.atan2(dir_y).to_degrees().rem_euclid(360.0));
    (value, dir)
}

fn destination(lat: f64, lon: f64, az_deg: f64, dist_m: f64) -> (f64, f64) {
    let d = dist_m / 6_371_000.0;
    let br = az_deg.to_radians();
    let p1 = lat.to_radians();
    let p2 = (p1.sin() * d.cos() + p1.cos() * d.sin() * br.cos()).asin();
    let l2 =
        lon.to_radians() + (br.sin() * d.sin() * p1.cos()).atan2(d.cos() - p1.sin() * p2.sin());
    (p2.to_degrees(), l2.to_degrees())
}

fn great_circle_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let (dp, dl) = ((lat2 - lat1).to_radians(), (lon2 - lon1).to_radians());
    let h = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * 6371.0 * h.clamp(0.0, 1.0).sqrt().asin()
}

fn compact_hour(time: &str) -> String {
    time.chars().filter(|c| c.is_ascii_alphanumeric()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_name_wind_pressure_water_and_altitudes() {
        let layers = layers();
        let codes: Vec<_> = layers.products.iter().map(|p| p.code.as_str()).collect();
        assert_eq!(codes, ["REF", "WIND", "PRES", "WATER", "TEMP", "PRECIP"]);
        let groups: Vec<_> = layers.sources.iter().map(|s| s.group.as_str()).collect();
        assert_eq!(
            layers
                .sources
                .iter()
                .map(|s| s.id.as_str())
                .collect::<Vec<_>>(),
            [
                "nexrad",
                "now",
                "gfs",
                "ecmwf",
                "wrf",
                "dmc",
                "dmc_wrf_gfs",
                "dmc_wrf_ecmwf",
                "cdo",
                "meteostat"
            ]
        );
        assert_eq!(
            groups,
            [
                "report", "report", "forecast", "forecast", "forecast", "report", "forecast",
                "forecast", "report", "report"
            ]
        );
        let wind = layers.products.iter().find(|p| p.code == "WIND").unwrap();
        assert_eq!(wind.altitudes.len(), 6);
        assert_eq!(wind.altitudes[2].hpa, 850);
        assert!(wind.altitudes[2].name.contains("5 000"));
    }

    #[test]
    fn hour_stamps_become_protocol_times() {
        assert_eq!(iso_hour("2026-09-10T20:00"), "2026-09-10T20:00:00Z");
        assert_eq!(iso_hour("2026-09-10T20:00:00Z"), "2026-09-10T20:00:00Z");
        assert_eq!(current_hour(&["2026-09-10T00:00".into()]), 0);
    }

    #[test]
    fn interpolating_santiago_wind_uses_the_nearest_samples() {
        let samples = vec![
            Sample {
                lat: -33.45,
                lon: -70.67,
                wind_kt: [Some(18.0), None, Some(32.0), None, None, None],
                wind_dir: [Some(180.0), None, Some(200.0), None, None, None],
                pressure_hpa: Some(1016.0),
                height_m: [None, None, Some(1500.0), None, None, None],
                water_pct: [Some(45.0), None, Some(70.0), None, None, None],
                ..Sample::default()
            },
            Sample {
                lat: -33.0,
                lon: -70.0,
                wind_kt: [Some(10.0), None, Some(20.0), None, None, None],
                wind_dir: [Some(90.0), None, Some(90.0), None, None, None],
                pressure_hpa: Some(1012.0),
                height_m: [None, None, Some(1480.0), None, None, None],
                water_pct: [Some(40.0), None, Some(60.0), None, None, None],
                ..Sample::default()
            },
        ];
        let (speed, dir) = interpolate(&samples, -33.45, -70.67, "WIND", 2);
        assert!((speed.unwrap() - 32.0).abs() < 2.0);
        assert!(dir.unwrap() > 170.0 && dir.unwrap() < 210.0);
        let (rh, _) = interpolate(&samples, -33.45, -70.67, "WATER", 2);
        assert!((rh.unwrap() - 70.0).abs() < 3.0);
        let (mslp, _) = interpolate(&samples, -33.45, -70.67, "PRES", 0);
        assert!((mslp.unwrap() - 1016.0).abs() < 1.0);
    }

    #[test]
    fn polar_raster_of_a_constant_field_fills_the_disk() {
        let samples = vec![Sample {
            lat: -33.45,
            lon: -70.67,
            wind_kt: [Some(12.0); 6],
            wind_dir: [Some(270.0); 6],
            pressure_hpa: Some(1013.0),
            height_m: [Some(0.0); 6],
            water_pct: [Some(55.0); 6],
            ..Sample::default()
        }];
        let (frame, texture, lut) = raster(
            &samples,
            -33.45,
            -70.67,
            "gfs",
            "WIND",
            ALTITUDES[2],
            "2026-09-10T20:00:00Z",
        )
        .unwrap();
        assert_eq!(frame.product, "WIND");
        assert_eq!(frame.layer_source, "gfs");
        assert_eq!(frame.kind, "field");
        assert_eq!(frame.altitude_hpa, 850);
        assert_eq!(frame.units, "kt");
        assert_eq!(frame.rays, 360);
        assert_eq!(frame.gates, 64);
        assert!(!texture.is_empty());
        assert!(!lut.is_empty());
    }
}

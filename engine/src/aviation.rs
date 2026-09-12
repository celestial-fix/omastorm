//! Aviation briefing for the view centre: METAR, TAF, and hazard bulletins
//! from NOAA's Aviation Weather Center. Live mode only; archived daemons
//! and ordinary checks never fetch. `OMASTORM_AVIATION_URL` overrides the
//! API root for development and tests.

use crate::protocol::{Aviation, AviationStatus, Bulletin, Hazard, Point};
use serde::Deserialize;
use std::{
    env, io,
    time::{Duration, Instant},
};

const DEFAULT_URL: &str = "https://aviationweather.gov/api/data";
const USER_AGENT: &str = concat!(
    "omastorm/",
    env!("CARGO_PKG_VERSION"),
    " (https://omastorm.com)"
);
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_BODY: usize = 2 << 20;
const METAR_HALF_DEG: f64 = 2.0;
const HAZARD_HALF_DEG: f64 = 5.0;
/// Skip a refresh when the centre has not moved this far and the last
/// successful fetch is still fresh.
const STILL_KM: f64 = 25.0;
const FRESH: Duration = Duration::from_secs(5 * 60);
const ATTRIBUTION: &str = "NOAA Aviation Weather Center";

pub struct Client {
    base: String,
    http: reqwest::Client,
    last: std::sync::Mutex<Option<Last>>,
}

struct Last {
    lat: f64,
    lon: f64,
    at: Instant,
}

impl Client {
    pub fn open() -> io::Result<Client> {
        let base = env::var("OMASTORM_AVIATION_URL").unwrap_or_else(|_| DEFAULT_URL.into());
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(FETCH_TIMEOUT)
            .build()
            .map_err(io::Error::other)?;
        Ok(Client {
            base: base.trim_end_matches('/').to_owned(),
            http,
            last: std::sync::Mutex::new(None),
        })
    }

    pub fn idle() -> Aviation {
        Aviation {
            status: AviationStatus::Idle,
            attribution: ATTRIBUTION.into(),
            station: None,
            metar: None,
            taf: None,
            hazards: Vec::new(),
            gramet: crate::gramet::idle(),
        }
    }

    pub fn loading() -> Aviation {
        Aviation {
            status: AviationStatus::Loading,
            attribution: ATTRIBUTION.into(),
            station: None,
            metar: None,
            taf: None,
            hazards: Vec::new(),
            gramet: crate::gramet::idle(),
        }
    }

    /// Whether a new centre is worth a fetch.
    pub fn should_refresh(&self, lat: f64, lon: f64) -> bool {
        match &*self.last.lock().unwrap() {
            None => true,
            Some(last) => {
                last.at.elapsed() >= FRESH
                    || great_circle_km(last.lat, last.lon, lat, lon) >= STILL_KM
            }
        }
    }

    pub async fn fetch(&self, lat: f64, lon: f64) -> Aviation {
        match self.load(lat, lon).await {
            Ok(briefing) => {
                *self.last.lock().unwrap() = Some(Last {
                    lat,
                    lon,
                    at: Instant::now(),
                });
                briefing
            }
            Err(e) => {
                eprintln!("Aviation briefing: {e}");
                Aviation {
                    status: AviationStatus::Offline,
                    attribution: ATTRIBUTION.into(),
                    station: None,
                    metar: None,
                    taf: None,
                    hazards: Vec::new(),
                    gramet: crate::gramet::idle(),
                }
            }
        }
    }

    async fn load(&self, lat: f64, lon: f64) -> io::Result<Aviation> {
        let metar_bbox = bbox(lat, lon, METAR_HALF_DEG);
        let hazard_bbox = bbox(lat, lon, HAZARD_HALF_DEG);
        let stations = self
            .get_json(&format!("/stationinfo?bbox={metar_bbox}&format=json"))
            .await
            .unwrap_or_else(|_| serde_json::json!([]));
        let identified = nearest_airport(&stations, lat, lon).or_else(|| {
            crate::airports::nearest(lat, lon).map(|(a, km)| Identified {
                icao: a.icao.into(),
                name: a.name.into(),
                lat: a.lat,
                lon: a.lon,
                distance_km: km,
            })
        });
        let mut metar = None;
        if let Some(id) = identified.as_ref().map(|s| s.icao.clone())
            && let Ok(body) = self.get_json(&format!("/metar?ids={id}&format=json")).await
        {
            metar = nearest_metar(&body, lat, lon).map(|m| bulletin_from_metar(&m));
        }
        if metar.is_none() {
            let metars = self
                .get_json(&format!("/metar?bbox={metar_bbox}&format=json"))
                .await?;
            metar = nearest_metar(&metars, lat, lon).map(|m| bulletin_from_metar(&m));
        }
        let station = identified
            .as_ref()
            .map(|s| crate::protocol::AviationStation {
                id: s.icao.clone(),
                name: s.name.clone(),
                lat: s.lat,
                lon: s.lon,
                distance_km: s.distance_km,
            });
        let taf = match station.as_ref() {
            Some(s) => {
                let body = self
                    .get_json(&format!("/taf?ids={}&format=json", s.id))
                    .await?;
                nearest_taf(&body, &s.id).map(bulletin_from_taf)
            }
            None => None,
        };
        let mut hazards = Vec::new();
        for (path, fallback_kind) in [
            (format!("/isigmet?bbox={hazard_bbox}&format=json"), "sigmet"),
            (
                format!("/airsigmet?bbox={hazard_bbox}&format=json"),
                "sigmet",
            ),
            (format!("/gairmet?bbox={hazard_bbox}&format=json"), "airmet"),
            (format!("/airmet?bbox={hazard_bbox}&format=json"), "airmet"),
        ] {
            match self.get_json(&path).await {
                Ok(body) => hazards.extend(hazards_from_json(&body, fallback_kind)),
                Err(e) => eprintln!("Aviation {path}: {e}"),
            }
        }
        hazards.retain(|h| hazard_near(h, lat, lon));
        let status = if station.is_none() && metar.is_none() && hazards.is_empty() {
            AviationStatus::Unavailable
        } else {
            AviationStatus::Ok
        };
        let attribution = if crate::airports::in_chile(lat, lon) {
            format!("{ATTRIBUTION} · Dirección Meteorológica de Chile")
        } else {
            ATTRIBUTION.into()
        };
        Ok(Aviation {
            status,
            attribution,
            station,
            metar,
            taf,
            hazards,
            gramet: crate::gramet::idle(),
        })
    }

    async fn get_json(&self, path: &str) -> io::Result<serde_json::Value> {
        let url = format!("{}{path}", self.base);
        let response = self.http.get(&url).send().await.map_err(io::Error::other)?;
        if !response.status().is_success() {
            return Err(io::Error::other(format!(
                "{url} returned {}",
                response.status()
            )));
        }
        let bytes = response.bytes().await.map_err(io::Error::other)?;
        if bytes.len() > MAX_BODY {
            return Err(io::Error::other(format!("{url} body too large")));
        }
        if bytes.is_empty() {
            return Ok(serde_json::json!([]));
        }
        serde_json::from_slice(&bytes).map_err(io::Error::other)
    }
}

fn bbox(lat: f64, lon: f64, half: f64) -> String {
    let lat0 = (lat - half).clamp(-90.0, 90.0);
    let lat1 = (lat + half).clamp(-90.0, 90.0);
    let lon0 = (lon - half).clamp(-180.0, 180.0);
    let lon1 = (lon + half).clamp(-180.0, 180.0);
    format!("{lat0:.4},{lon0:.4},{lat1:.4},{lon1:.4}")
}

fn great_circle_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let (dp, dl) = ((lat2 - lat1).to_radians(), (lon2 - lon1).to_radians());
    let h = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * 6371.0 * h.clamp(0.0, 1.0).sqrt().asin()
}

struct Identified {
    icao: String,
    name: String,
    lat: f64,
    lon: f64,
    distance_km: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StationInfo {
    #[serde(default)]
    icao_id: String,
    #[serde(default)]
    site: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    lat: f64,
    #[serde(default)]
    lon: f64,
}

fn nearest_airport(value: &serde_json::Value, lat: f64, lon: f64) -> Option<Identified> {
    as_records::<StationInfo>(value)
        .into_iter()
        .filter(|s| s.icao_id.len() == 4 && s.lat.abs() <= 90.0 && s.lon.abs() <= 180.0)
        .map(|s| {
            let name = if !s.site.is_empty() { s.site } else { s.name };
            Identified {
                distance_km: great_circle_km(lat, lon, s.lat, s.lon),
                icao: s.icao_id,
                name,
                lat: s.lat,
                lon: s.lon,
            }
        })
        .min_by(|a, b| a.distance_km.total_cmp(&b.distance_km))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MetarRecord {
    #[serde(default)]
    icao_id: String,
    #[serde(default)]
    lat: f64,
    #[serde(default)]
    lon: f64,
    #[serde(default)]
    raw_ob: String,
    #[serde(default)]
    obs_time: Option<i64>,
    #[serde(default)]
    report_time: String,
    #[serde(default)]
    flt_cat: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TafRecord {
    #[serde(default)]
    icao_id: String,
    #[serde(default)]
    raw_taf: String,
    #[serde(default)]
    issue_time: String,
    #[serde(default)]
    valid_time_from: Option<i64>,
    #[serde(default)]
    valid_time_to: Option<i64>,
}

fn as_records<T: for<'de> Deserialize<'de>>(value: &serde_json::Value) -> Vec<T> {
    match value {
        serde_json::Value::Array(items) => items
            .iter()
            .filter_map(|item| serde_json::from_value(item.clone()).ok())
            .collect(),
        _ => Vec::new(),
    }
}

fn nearest_metar(value: &serde_json::Value, lat: f64, lon: f64) -> Option<MetarRecord> {
    as_records::<MetarRecord>(value)
        .into_iter()
        .filter(|m| !m.icao_id.is_empty() && !m.raw_ob.is_empty())
        .min_by(|a, b| {
            great_circle_km(lat, lon, a.lat, a.lon)
                .total_cmp(&great_circle_km(lat, lon, b.lat, b.lon))
        })
}

fn nearest_taf(value: &serde_json::Value, id: &str) -> Option<TafRecord> {
    as_records::<TafRecord>(value)
        .into_iter()
        .find(|t| t.icao_id == id && !t.raw_taf.is_empty())
}

fn bulletin_from_metar(record: &MetarRecord) -> Bulletin {
    Bulletin {
        raw: record.raw_ob.clone(),
        time: unix_or_text(record.obs_time, &record.report_time),
        valid_from: String::new(),
        valid_to: String::new(),
        category: record.flt_cat.clone(),
    }
}

fn bulletin_from_taf(record: TafRecord) -> Bulletin {
    Bulletin {
        raw: record.raw_taf,
        time: iso_from_awc(&record.issue_time),
        valid_from: unix_iso(record.valid_time_from),
        valid_to: unix_iso(record.valid_time_to),
        category: String::new(),
    }
}

fn hazards_from_json(value: &serde_json::Value, fallback_kind: &str) -> Vec<Hazard> {
    let serde_json::Value::Array(items) = value else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| hazard_from_value(item, fallback_kind))
        .collect()
}

fn hazard_from_value(value: &serde_json::Value, fallback_kind: &str) -> Option<Hazard> {
    let raw = first_string(
        value,
        &["rawAirSigmet", "rawSigmet", "rawAirmet", "raw", "rawOb"],
    );
    if raw.is_empty()
        && value
            .get("hazard")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .is_empty()
    {
        return None;
    }
    let kind = if looks_like_word(&raw, "GRAMET") {
        "gramet"
    } else if looks_like_word(&raw, "GAMET") {
        "gamet"
    } else if fallback_kind == "airmet"
        || value
            .get("product")
            .and_then(|v| v.as_str())
            .is_some_and(|p| matches!(p, "SIERRA" | "TANGO" | "ZULU"))
        || value
            .get("airSigmetType")
            .and_then(|v| v.as_str())
            .is_some_and(|t| t.eq_ignore_ascii_case("airmet"))
    {
        "airmet"
    } else {
        "sigmet"
    };
    let hazard = value
        .get("hazard")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();
    Some(Hazard {
        kind: kind.into(),
        hazard,
        raw,
        coords: coords_from_value(value.get("coords")),
        valid_from: unix_iso(value.get("validTimeFrom").and_then(|v| v.as_i64())),
        valid_to: unix_iso(value.get("validTimeTo").and_then(|v| v.as_i64())),
    })
}

fn looks_like_word(raw: &str, needle: &str) -> bool {
    raw.split(|c: char| !c.is_ascii_alphanumeric())
        .any(|word| word.eq_ignore_ascii_case(needle))
}

fn looks_like_area_forecast(raw: &str) -> bool {
    looks_like_word(raw, "GAMET") || looks_like_word(raw, "GRAMET")
}

fn coords_from_value(value: Option<&serde_json::Value>) -> Vec<Point> {
    let Some(serde_json::Value::Array(items)) = value else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let lat = item.get("lat").and_then(|v| v.as_f64())?;
            let lon = item.get("lon").and_then(|v| v.as_f64())?;
            if (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon) {
                Some(Point { lat, lon })
            } else {
                None
            }
        })
        .collect()
}

fn hazard_near(hazard: &Hazard, lat: f64, lon: f64) -> bool {
    if hazard.coords.is_empty() {
        return looks_like_area_forecast(&hazard.raw);
    }
    hazard
        .coords
        .iter()
        .any(|p| great_circle_km(lat, lon, p.lat, p.lon) <= 800.0)
}

fn first_string(value: &serde_json::Value, keys: &[&str]) -> String {
    for key in keys {
        if let Some(text) = value.get(*key).and_then(|v| v.as_str())
            && !text.is_empty()
        {
            return text.to_owned();
        }
    }
    String::new()
}

fn unix_or_text(unix: Option<i64>, text: &str) -> String {
    let iso = unix_iso(unix);
    if !iso.is_empty() {
        iso
    } else {
        iso_from_awc(text)
    }
}

fn unix_iso(unix: Option<i64>) -> String {
    let Some(seconds) = unix else {
        return String::new();
    };
    chrono::DateTime::from_timestamp(seconds, 0)
        .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_default()
}

fn iso_from_awc(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let normalized = trimmed.replace(' ', "T");
    let candidate =
        if normalized.contains('T') && !normalized.ends_with('Z') && !normalized.contains('+') {
            format!("{normalized}Z")
        } else {
            normalized
        };
    chrono::DateTime::parse_from_rfc3339(&candidate)
        .map(|t| {
            t.with_timezone(&chrono::Utc)
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
        })
        .unwrap_or_else(|_| trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tobalaba_identifies_sctb_even_when_only_scel_has_a_live_metar() {
        let stations = serde_json::json!([
            {
                "icaoId": "SCEL",
                "site": "Santiago/Benítez Intl",
                "lat": -33.393,
                "lon": -70.786
            },
            {
                "icaoId": "SCTB",
                "site": "Santiago/Sanchez Arpt",
                "lat": -33.456,
                "lon": -70.547
            }
        ]);
        let found = nearest_airport(&stations, -33.45, -70.55).unwrap();
        assert_eq!(found.icao, "SCTB");
        assert!(found.name.contains("Sanchez"));
        assert!(found.distance_km < 8.0);
    }

    #[test]
    fn santiago_metar_is_the_nearest_in_the_box() {
        let body = serde_json::json!([
            {
                "icaoId": "SCEL",
                "lat": -33.393,
                "lon": -70.786,
                "rawOb": "SCEL 101900Z 18008KT 9999 FEW030 18/08 Q1016",
                "obsTime": 1694365200,
                "fltCat": "VFR"
            },
            {
                "icaoId": "SCVM",
                "lat": -32.95,
                "lon": -71.48,
                "rawOb": "SCVM 101900Z 20010KT CAVOK 16/10 Q1015",
                "obsTime": 1694365200
            }
        ]);
        let nearest = nearest_metar(&body, -33.45, -70.67).unwrap();
        assert_eq!(nearest.icao_id, "SCEL");
        let briefing = bulletin_from_metar(&nearest);
        assert!(briefing.raw.starts_with("SCEL"));
        assert_eq!(briefing.category, "VFR");
        assert_eq!(briefing.time, "2023-09-10T17:00:00Z");
    }

    #[test]
    fn gamet_and_sigmet_kinds_are_taken_from_the_bulletin() {
        let sigmet = hazard_from_value(
            &serde_json::json!({
                "hazard": "TURB",
                "rawSigmet": "WSCH31 SCFA 101200\nSIGMET 3 VALID 101200/101600 SCFA-\nSANTIAGO FIR SEV TURB FCST",
                "coords": [{"lat": -33.2, "lon": -70.8}, {"lat": -34.0, "lon": -70.1}],
                "validTimeFrom": 1694347200,
                "validTimeTo": 1694361600
            }),
            "sigmet",
        )
        .unwrap();
        assert_eq!(sigmet.kind, "sigmet");
        assert_eq!(sigmet.hazard, "TURB");
        assert_eq!(sigmet.coords.len(), 2);
        assert!(hazard_near(&sigmet, -33.45, -70.67));

        let gamet = hazard_from_value(
            &serde_json::json!({
                "raw": "GAMET VALID 101200/101800 SCFA-\nSANTIAGO FIR\nSECN I\nSFC WIND: 15KT"
            }),
            "sigmet",
        )
        .unwrap();
        assert_eq!(gamet.kind, "gamet");
        assert!(hazard_near(&gamet, -33.45, -70.67));

        let gramet = hazard_from_value(
            &serde_json::json!({
                "raw": "GRAMET VALID 101200/101800 SCEL-\nSANTIAGO ROUTE FL100/350"
            }),
            "sigmet",
        )
        .unwrap();
        assert_eq!(gramet.kind, "gramet");
        assert!(hazard_near(&gramet, -33.45, -70.67));
    }

    #[test]
    fn bbox_stays_inside_the_globe() {
        let box_santiago = bbox(-33.45, -70.67, 2.0);
        assert_eq!(box_santiago, "-35.4500,-72.6700,-31.4500,-68.6700");
        let polar = bbox(89.0, 10.0, 5.0);
        assert!(polar.starts_with("84.0000"));
        assert!(polar.contains(",90.0000,"));
    }
}

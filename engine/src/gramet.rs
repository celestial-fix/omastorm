//! Route GRAMET: origin and destination ICAO plus cruise TAS (and an
//! optional flight level). Live mode resolves the airports from NOAA AWC
//! stationinfo and samples the current Open-Meteo hour along the
//! great-circle. The result is issued-style bulletin text and a route
//! polyline — not a GRIB file. Archived daemons never fetch.
//! `OMASTORM_FIELDS_URL` overrides the weather API; airports use the
//! aviation root (`OMASTORM_AVIATION_URL`).

use crate::protocol::{AviationStatus, Gramet, GrametFix, GrametLeg, Point};
use serde::Deserialize;
use std::{env, io};

const FIELDS_URL: &str = "https://api.open-meteo.com/v1/forecast";
const USER_AGENT: &str = concat!(
    "omastorm/",
    env!("CARGO_PKG_VERSION"),
    " (https://omastorm.com)"
);
const FETCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
const MAX_BODY: usize = 2 << 20;
const SAMPLES: usize = 9;
const MIN_TAS: u32 = 80;
const MAX_TAS: u32 = 550;
const DEFAULT_FL: u32 = 350;
const MIN_FL: u32 = 50;
const MAX_FL: u32 = 450;

#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub origin: String,
    pub destination: String,
    pub cruise_kt: u32,
    pub flight_level: u32,
}

pub fn idle() -> Gramet {
    Gramet {
        status: AviationStatus::Idle,
        origin: None,
        destination: None,
        cruise_kt: 0,
        flight_level: DEFAULT_FL,
        distance_km: 0.0,
        ete_min: 0,
        raw: String::new(),
        coords: Vec::new(),
        legs: Vec::new(),
        message: String::new(),
    }
}

pub fn loading(request: &Request) -> Gramet {
    Gramet {
        status: AviationStatus::Loading,
        origin: Some(fix(&request.origin, "", 0.0, 0.0)),
        destination: Some(fix(&request.destination, "", 0.0, 0.0)),
        cruise_kt: request.cruise_kt,
        flight_level: request.flight_level,
        distance_km: 0.0,
        ete_min: 0,
        raw: String::new(),
        coords: Vec::new(),
        legs: Vec::new(),
        message: String::new(),
    }
}

pub fn parse_request(
    origin: &str,
    destination: &str,
    cruise_kt: u32,
    flight_level: u32,
) -> Result<Request, String> {
    let origin =
        icao(origin).ok_or_else(|| String::from("GRAMET origin must be a four-letter ICAO id."))?;
    let destination = icao(destination)
        .ok_or_else(|| String::from("GRAMET destination must be a four-letter ICAO id."))?;
    if origin == destination {
        return Err("GRAMET origin and destination must differ.".into());
    }
    if !(MIN_TAS..=MAX_TAS).contains(&cruise_kt) {
        return Err(format!(
            "GRAMET cruise speed must be {MIN_TAS}–{MAX_TAS} kt."
        ));
    }
    let flight_level = if flight_level == 0 {
        DEFAULT_FL
    } else {
        flight_level
    };
    if !(MIN_FL..=MAX_FL).contains(&flight_level) {
        return Err(format!("GRAMET flight level must be {MIN_FL}–{MAX_FL}."));
    }
    Ok(Request {
        origin,
        destination,
        cruise_kt,
        flight_level,
    })
}

fn icao(text: &str) -> Option<String> {
    let trimmed = text.trim().to_ascii_uppercase();
    (trimmed.len() == 4 && trimmed.chars().all(|c| c.is_ascii_alphabetic())).then_some(trimmed)
}

/// Pressure level used for the cruise sample. Open-Meteo names these
/// `wind_speed_{hPa}hPa` / `temperature_{hPa}hPa`.
pub fn cruise_hpa(flight_level: u32) -> u32 {
    match flight_level {
        0..=50 => 850,
        51..=80 => 700,
        81..=140 => 500,
        141..=200 => 400,
        201..=280 => 300,
        281..=360 => 250,
        _ => 200,
    }
}

fn fix(icao: &str, name: &str, lat: f64, lon: f64) -> GrametFix {
    GrametFix {
        icao: icao.into(),
        name: name.into(),
        lat,
        lon,
    }
}

pub struct Client {
    aviation: String,
    fields: String,
    http: reqwest::Client,
}

impl Client {
    pub fn open() -> io::Result<Client> {
        let aviation = env::var("OMASTORM_AVIATION_URL")
            .unwrap_or_else(|_| "https://aviationweather.gov/api/data".into());
        let fields = env::var("OMASTORM_FIELDS_URL").unwrap_or_else(|_| FIELDS_URL.into());
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(FETCH_TIMEOUT)
            .build()
            .map_err(io::Error::other)?;
        Ok(Client {
            aviation: aviation.trim_end_matches('/').to_owned(),
            fields: fields.trim_end_matches('/').to_owned(),
            http,
        })
    }

    pub async fn fetch(&self, request: &Request) -> Gramet {
        match self.load(request).await {
            Ok(gramet) => gramet,
            Err(e) => {
                eprintln!("GRAMET: {e}");
                Gramet {
                    status: AviationStatus::Offline,
                    origin: Some(fix(&request.origin, "", 0.0, 0.0)),
                    destination: Some(fix(&request.destination, "", 0.0, 0.0)),
                    cruise_kt: request.cruise_kt,
                    flight_level: request.flight_level,
                    distance_km: 0.0,
                    ete_min: 0,
                    raw: String::new(),
                    coords: Vec::new(),
                    legs: Vec::new(),
                    message: e.to_string(),
                }
            }
        }
    }

    async fn load(&self, request: &Request) -> io::Result<Gramet> {
        let origin = self.airport(&request.origin).await?;
        let destination = self.airport(&request.destination).await?;
        let distance_km = great_circle_km(origin.lat, origin.lon, destination.lat, destination.lon);
        if distance_km < 1.0 {
            return Err(io::Error::other(
                "GRAMET airports are too close to plot a route",
            ));
        }
        let tas_kmh = f64::from(request.cruise_kt) * 1.852;
        let ete_min = ((distance_km / tas_kmh) * 60.0).round().max(1.0) as u32;
        let mut lats = Vec::new();
        let mut lons = Vec::new();
        for i in 0..SAMPLES {
            let t = i as f64 / (SAMPLES - 1) as f64;
            let (lat, lon) =
                intermediate(origin.lat, origin.lon, destination.lat, destination.lon, t);
            lats.push(lat);
            lons.push(lon);
        }
        let hpa = cruise_hpa(request.flight_level);
        let samples = self.along_route(&lats, &lons, hpa).await?;
        let legs = legs_from_samples(
            &lats,
            &lons,
            &samples,
            distance_km,
            ete_min,
            request.cruise_kt,
        );
        let coords = lats
            .iter()
            .zip(&lons)
            .map(|(&lat, &lon)| Point { lat, lon })
            .collect();
        let raw = bulletin(
            request,
            &origin,
            &destination,
            distance_km,
            ete_min,
            hpa,
            &legs,
        );
        Ok(Gramet {
            status: AviationStatus::Ok,
            origin: Some(origin),
            destination: Some(destination),
            cruise_kt: request.cruise_kt,
            flight_level: request.flight_level,
            distance_km: (distance_km * 10.0).round() / 10.0,
            ete_min,
            raw,
            coords,
            legs,
            message: String::new(),
        })
    }

    async fn airport(&self, icao: &str) -> io::Result<GrametFix> {
        let url = format!("{}/stationinfo?ids={icao}&format=json", self.aviation);
        let value = self.get_json(&url).await?;
        parse_airport(&value, icao)
            .ok_or_else(|| io::Error::other(format!("no AWC stationinfo for {icao}")))
    }

    async fn along_route(
        &self,
        lats: &[f64],
        lons: &[f64],
        hpa: u32,
    ) -> io::Result<Vec<RouteSample>> {
        let hourly = format!(
            "wind_speed_{hpa}hPa,wind_direction_{hpa}hPa,temperature_{hpa}hPa,relative_humidity_{hpa}hPa"
        );
        let url = format!(
            "{}?latitude={}&longitude={}&hourly={}&wind_speed_unit=kn&forecast_days=1&timezone=UTC",
            self.fields,
            lats.iter()
                .map(|v| format!("{v:.4}"))
                .collect::<Vec<_>>()
                .join(","),
            lons.iter()
                .map(|v| format!("{v:.4}"))
                .collect::<Vec<_>>()
                .join(","),
            hourly
        );
        let value = self.get_json(&url).await?;
        parse_route_weather(&value, lats.len())
    }

    async fn get_json(&self, url: &str) -> io::Result<serde_json::Value> {
        let response = self.http.get(url).send().await.map_err(io::Error::other)?;
        if !response.status().is_success() {
            return Err(io::Error::other(format!(
                "{url} returned {}",
                response.status()
            )));
        }
        let bytes = response.bytes().await.map_err(io::Error::other)?;
        if bytes.len() > MAX_BODY {
            return Err(io::Error::other("GRAMET body too large"));
        }
        if bytes.is_empty() {
            return Ok(serde_json::json!([]));
        }
        serde_json::from_slice(&bytes).map_err(io::Error::other)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StationInfo {
    #[serde(default)]
    icao_id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    lat: f64,
    #[serde(default)]
    lon: f64,
}

fn parse_airport(value: &serde_json::Value, icao: &str) -> Option<GrametFix> {
    let items = match value {
        serde_json::Value::Array(items) => items.clone(),
        obj if obj.is_object() => vec![obj.clone()],
        _ => return None,
    };
    items.iter().find_map(|item| {
        let info: StationInfo = serde_json::from_value(item.clone()).ok()?;
        let id = info.icao_id.to_ascii_uppercase();
        if id != icao {
            return None;
        }
        if !(-90.0..=90.0).contains(&info.lat) || !(-180.0..=180.0).contains(&info.lon) {
            return None;
        }
        Some(fix(&id, &info.name, info.lat, info.lon))
    })
}

#[derive(Clone, Copy, Default)]
struct RouteSample {
    wind_kt: Option<f64>,
    wind_dir: Option<f64>,
    temp_c: Option<f64>,
    rh: Option<f64>,
}

fn parse_route_weather(value: &serde_json::Value, n: usize) -> io::Result<Vec<RouteSample>> {
    let places: Vec<serde_json::Value> = if value.is_array() {
        serde_json::from_value(value.clone()).map_err(io::Error::other)?
    } else {
        vec![value.clone()]
    };
    if places.is_empty() {
        return Err(io::Error::other("GRAMET weather response is empty"));
    }
    let mut out = vec![RouteSample::default(); n];
    for (i, place) in places.iter().enumerate().take(n) {
        let Some(hourly) = place.get("hourly") else {
            continue;
        };
        let times = hourly
            .get("time")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let hour = current_hour(&times);
        let keys: Vec<String> = hourly
            .as_object()
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default();
        out[i] = RouteSample {
            wind_kt: first_hourly(hourly, &keys, "wind_speed_", hour),
            wind_dir: first_hourly(hourly, &keys, "wind_direction_", hour),
            temp_c: first_hourly(hourly, &keys, "temperature_", hour),
            rh: first_hourly(hourly, &keys, "relative_humidity_", hour),
        };
    }
    Ok(out)
}

fn first_hourly(
    hourly: &serde_json::Value,
    keys: &[String],
    prefix: &str,
    hour: usize,
) -> Option<f64> {
    let key = keys
        .iter()
        .find(|k| k.starts_with(prefix) && k.as_str() != "time")?;
    hourly
        .get(key)
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.get(hour))
        .and_then(|v| v.as_f64())
}

fn current_hour(times: &[serde_json::Value]) -> usize {
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:00").to_string();
    times
        .iter()
        .enumerate()
        .rev()
        .find_map(|(i, v)| {
            let t = v.as_str()?;
            (t.replace('Z', "").starts_with(&now) || t <= now.as_str()).then_some(i)
        })
        .unwrap_or(0)
}

fn legs_from_samples(
    lats: &[f64],
    lons: &[f64],
    samples: &[RouteSample],
    distance_km: f64,
    ete_min: u32,
    _cruise_kt: u32,
) -> Vec<GrametLeg> {
    let n = lats.len().max(1);
    lats.iter()
        .zip(lons)
        .enumerate()
        .map(|(i, (&lat, &lon))| {
            let frac = i as f64 / (n - 1).max(1) as f64;
            let sample = samples.get(i).copied().unwrap_or_default();
            GrametLeg {
                dist_km: (distance_km * frac * 10.0).round() / 10.0,
                ete_min: ((f64::from(ete_min) * frac).round() as u32),
                lat,
                lon,
                wind_kt: sample.wind_kt.unwrap_or(0.0),
                wind_dir: sample.wind_dir.unwrap_or(0.0),
                temp_c: sample.temp_c.unwrap_or(0.0),
                rh: sample.rh.unwrap_or(0.0),
            }
        })
        .collect()
}

fn bulletin(
    request: &Request,
    origin: &GrametFix,
    destination: &GrametFix,
    distance_km: f64,
    ete_min: u32,
    hpa: u32,
    legs: &[GrametLeg],
) -> String {
    let now = chrono::Utc::now();
    let start = now.format("%d%H%M").to_string();
    let end = (now + chrono::Duration::minutes(i64::from(ete_min)))
        .format("%d%H%M")
        .to_string();
    let hours = ete_min / 60;
    let mins = ete_min % 60;
    let mut lines = vec![
        format!(
            "GRAMET {}-{} TAS{}KT FL{}",
            request.origin, request.destination, request.cruise_kt, request.flight_level
        ),
        format!(
            "DIST {:.0}KM ETE {hours}H{mins:02} CRUISE {hpa}HPA",
            distance_km
        ),
        format!("VALID {start}/{end}"),
        format!(
            "{} {} {:.2} {:.2}",
            origin.icao,
            origin.name.to_ascii_uppercase(),
            origin.lat,
            origin.lon
        ),
    ];
    for (i, leg) in legs.iter().enumerate() {
        if i == 0 || i + 1 == legs.len() {
            continue;
        }
        lines.push(format!(
            "{:4.0}KM /{:03} {:03.0}/{:02.0}KT {:+.0}C RH{:.0}",
            leg.dist_km, leg.ete_min, leg.wind_dir, leg.wind_kt, leg.temp_c, leg.rh
        ));
    }
    lines.push(format!(
        "{} {} {:.2} {:.2}",
        destination.icao,
        destination.name.to_ascii_uppercase(),
        destination.lat,
        destination.lon
    ));
    lines.join("\n")
}

fn great_circle_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let (dp, dl) = ((lat2 - lat1).to_radians(), (lon2 - lon1).to_radians());
    let h = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * 6371.0 * h.clamp(0.0, 1.0).sqrt().asin()
}

fn intermediate(lat1: f64, lon1: f64, lat2: f64, lon2: f64, frac: f64) -> (f64, f64) {
    let p1 = lat1.to_radians();
    let p2 = lat2.to_radians();
    let l1 = lon1.to_radians();
    let l2 = lon2.to_radians();
    let d = {
        let (dp, dl) = (p2 - p1, l2 - l1);
        let h = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
        2.0 * h.clamp(0.0, 1.0).sqrt().asin()
    };
    if d < 1e-9 {
        return (lat1, lon1);
    }
    let a = ((1.0 - frac) * d).sin() / d.sin();
    let b = (frac * d).sin() / d.sin();
    let x = a * p1.cos() * l1.cos() + b * p2.cos() * l2.cos();
    let y = a * p1.cos() * l1.sin() + b * p2.cos() * l2.sin();
    let z = a * p1.sin() + b * p2.sin();
    (
        z.atan2((x * x + y * y).sqrt()).to_degrees(),
        y.atan2(x).to_degrees(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_needs_two_icao_and_cruise_tas() {
        let ok = parse_request("scel", "scfa", 420, 0).unwrap();
        assert_eq!(ok.origin, "SCEL");
        assert_eq!(ok.destination, "SCFA");
        assert_eq!(ok.cruise_kt, 420);
        assert_eq!(ok.flight_level, 350);
        assert!(parse_request("SCEL", "SCEL", 420, 350).is_err());
        assert!(parse_request("SCL", "SCFA", 420, 350).is_err());
        assert!(parse_request("SCEL", "SCFA", 20, 350).is_err());
    }

    #[test]
    fn cruise_level_maps_fl350_to_250_hpa() {
        assert_eq!(cruise_hpa(350), 250);
        assert_eq!(cruise_hpa(180), 400);
        assert_eq!(cruise_hpa(80), 700);
    }

    #[test]
    fn scel_to_scfa_is_about_a_thousand_kilometres() {
        let km = great_circle_km(-33.393, -70.786, -23.444, -70.443);
        assert!(km > 1000.0 && km < 1200.0, "{km}");
        let mid = intermediate(-33.393, -70.786, -23.444, -70.443, 0.5);
        assert!(mid.0 > -30.0 && mid.0 < -26.0);
    }

    #[test]
    fn stationinfo_reads_icao_name_and_coordinates() {
        let body = serde_json::json!([{
            "icaoId": "SCEL",
            "name": "Arturo Merino Benitez",
            "lat": -33.393,
            "lon": -70.786
        }]);
        let fix = parse_airport(&body, "SCEL").unwrap();
        assert_eq!(fix.icao, "SCEL");
        assert!(fix.name.contains("Merino"));
        assert!((fix.lat + 33.393).abs() < 0.001);
    }

    #[test]
    fn bulletin_names_origin_destination_tas_and_level() {
        let request = parse_request("SCEL", "SCFA", 420, 350).unwrap();
        let origin = fix("SCEL", "Santiago", -33.39, -70.79);
        let dest = fix("SCFA", "Antofagasta", -23.44, -70.44);
        let mid = GrametLeg {
            dist_km: 550.0,
            ete_min: 49,
            lat: -28.4,
            lon: -70.6,
            wind_kt: 38.0,
            wind_dir: 250.0,
            temp_c: -48.0,
            rh: 30.0,
        };
        let ends = GrametLeg {
            dist_km: 0.0,
            ete_min: 0,
            lat: origin.lat,
            lon: origin.lon,
            wind_kt: 0.0,
            wind_dir: 0.0,
            temp_c: 0.0,
            rh: 0.0,
        };
        let legs = vec![ends.clone(), mid, ends];
        let raw = bulletin(&request, &origin, &dest, 1100.0, 98, 250, &legs);
        assert!(raw.starts_with("GRAMET SCEL-SCFA TAS420KT FL350"));
        assert!(raw.contains("DIST 1100KM"));
        assert!(raw.contains("SCEL SANTIAGO"));
        assert!(raw.contains("SCFA ANTOFAGASTA"));
        assert!(raw.contains("250/38KT"));
    }
}

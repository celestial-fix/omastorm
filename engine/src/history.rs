//! Historical report layers: NOAA CDO (NCEI daily-summaries) and
//! Meteostat (hourly station dumps). Live mode only; archived daemons
//! never fetch. Time travel is a date (CDO) or an hour (Meteostat).
//! `OMASTORM_CDO_URL` and `OMASTORM_METEOSTAT_URL` override the roots.
//! Station lists and yearly CSVs cache under `$XDG_CACHE_HOME/omastorm/`.

use crate::fields::{self, Sample};
use crate::protocol::{AviationStatus, Frame, History, HistoryStation};
use chrono::{Duration, NaiveDate, NaiveDateTime, Utc};
use flate2::read::GzDecoder;
use serde::Deserialize;
use std::{env, fs, io, io::Read, path::PathBuf, time::Duration as StdDuration};

const CDO_URL: &str = "https://www.ncei.noaa.gov/access/services/data/v1";
const METEO_URL: &str = "https://data.meteostat.net";
const METEO_STATIONS: &str = "https://bulk.meteostat.net/v2/stations/lite.json.gz";
const USER_AGENT: &str = concat!(
    "omastorm/",
    env!("CARGO_PKG_VERSION"),
    " (https://omastorm.com)"
);
const FETCH_TIMEOUT: StdDuration = StdDuration::from_secs(20);
const MAX_BODY: usize = 8 << 20;
const HALF_DEG: f64 = 1.5;
const NEAREST: usize = 8;
const ATTRIB_CDO: &str = "NOAA NCEI Climate Data Online";
const ATTRIB_METEO: &str = "Meteostat";

pub fn idle() -> History {
    History {
        status: AviationStatus::Idle,
        source: String::new(),
        time: String::new(),
        step: "day".into(),
        attribution: format!("{ATTRIB_CDO} · {ATTRIB_METEO}"),
        stations: Vec::new(),
        message: String::new(),
    }
}

pub fn loading(source: &str, time: &str) -> History {
    History {
        status: AviationStatus::Loading,
        source: source.into(),
        time: time.into(),
        step: step_of(source).into(),
        attribution: attribution(source).into(),
        stations: Vec::new(),
        message: String::new(),
    }
}

pub fn step_of(source: &str) -> &'static str {
    if source == "meteostat" { "hour" } else { "day" }
}

fn attribution(source: &str) -> &'static str {
    if source == "meteostat" {
        ATTRIB_METEO
    } else {
        ATTRIB_CDO
    }
}

pub fn default_time(source: &str) -> String {
    let now = Utc::now();
    if source == "meteostat" {
        (now - Duration::hours(36))
            .format("%Y-%m-%dT%H:00:00Z")
            .to_string()
    } else {
        (now - Duration::days(2)).format("%Y-%m-%d").to_string()
    }
}

pub fn step_time(source: &str, time: &str, delta: i64) -> Result<String, String> {
    if delta == 0 {
        return Ok(normalize_time(source, time));
    }
    if source == "meteostat" {
        let t =
            parse_hour(time).ok_or_else(|| String::from("Meteostat time must be an ISO hour."))?;
        let next = t + Duration::hours(delta);
        let latest = Utc::now().naive_utc() - Duration::hours(24);
        let earliest =
            NaiveDateTime::parse_from_str("1970-01-01T00:00:00", "%Y-%m-%dT%H:%M:%S").unwrap();
        let clamped = next.min(latest).max(earliest);
        Ok(clamped.format("%Y-%m-%dT%H:00:00Z").to_string())
    } else {
        let day = parse_day(time).ok_or_else(|| String::from("CDO time must be an ISO date."))?;
        let next = day + Duration::days(delta);
        let latest = (Utc::now() - Duration::days(1)).date_naive();
        let earliest = NaiveDate::from_ymd_opt(1940, 1, 1).unwrap();
        let clamped = next.min(latest).max(earliest);
        Ok(clamped.format("%Y-%m-%d").to_string())
    }
}

pub fn normalize_time(source: &str, time: &str) -> String {
    if time.trim().is_empty() {
        return default_time(source);
    }
    step_time(source, time, 0).unwrap_or_else(|_| default_time(source))
}

fn parse_day(time: &str) -> Option<NaiveDate> {
    let trimmed = time.trim().trim_end_matches('Z');
    NaiveDate::parse_from_str(&trimmed[..trimmed.len().min(10)], "%Y-%m-%d").ok()
}

fn parse_hour(time: &str) -> Option<NaiveDateTime> {
    let trimmed = time.trim().replace(' ', "T").replace('Z', "");
    let stamp = if trimmed.len() >= 16 {
        format!("{}:00", &trimmed[..16])
    } else if trimmed.len() == 10 {
        format!("{trimmed}T00:00:00")
    } else {
        trimmed
    };
    NaiveDateTime::parse_from_str(&stamp, "%Y-%m-%dT%H:%M:%S").ok()
}

pub struct Client {
    cdo: String,
    meteo: String,
    stations: String,
    http: reqwest::Client,
}

impl Client {
    pub fn open() -> io::Result<Client> {
        let cdo = env::var("OMASTORM_CDO_URL").unwrap_or_else(|_| CDO_URL.into());
        let meteo = env::var("OMASTORM_METEOSTAT_URL").unwrap_or_else(|_| METEO_URL.into());
        let stations =
            env::var("OMASTORM_METEOSTAT_STATIONS").unwrap_or_else(|_| METEO_STATIONS.into());
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(FETCH_TIMEOUT)
            .build()
            .map_err(io::Error::other)?;
        Ok(Client {
            cdo: cdo.trim_end_matches('/').to_owned(),
            meteo: meteo.trim_end_matches('/').to_owned(),
            stations,
            http,
        })
    }

    pub async fn fetch(
        &self,
        lat: f64,
        lon: f64,
        source: &str,
        time: &str,
        product: &str,
    ) -> io::Result<(History, Frame, Vec<u8>, Vec<u8>)> {
        let time = normalize_time(source, time);
        let (stations, samples) = if source == "meteostat" {
            self.load_meteostat(lat, lon, &time).await?
        } else {
            self.load_cdo(lat, lon, &time).await?
        };
        let product = if fields::is_field(product) && product != "REF" {
            product
        } else {
            "TEMP"
        };
        let altitude = fields::altitude(0).unwrap();
        let iso = if source == "meteostat" {
            time.clone()
        } else {
            format!("{time}T00:00:00Z")
        };
        let (frame, texture, lut) =
            fields::raster(&samples, lat, lon, source, product, altitude, &iso)?;
        Ok((
            History {
                status: if samples.is_empty() {
                    AviationStatus::Unavailable
                } else {
                    AviationStatus::Ok
                },
                source: source.into(),
                time,
                step: step_of(source).into(),
                attribution: attribution(source).into(),
                stations,
                message: String::new(),
            },
            frame,
            texture,
            lut,
        ))
    }

    async fn load_cdo(
        &self,
        lat: f64,
        lon: f64,
        day: &str,
    ) -> io::Result<(Vec<HistoryStation>, Vec<Sample>)> {
        let north = (lat + HALF_DEG).clamp(-90.0, 90.0);
        let south = (lat - HALF_DEG).clamp(-90.0, 90.0);
        let east = (lon + HALF_DEG).clamp(-180.0, 180.0);
        let west = (lon - HALF_DEG).clamp(-180.0, 180.0);
        let url = format!(
            "{}?dataset=daily-summaries&dataTypes=TAVG,TMAX,TMIN,PRCP,AWND&startDate={day}&endDate={day}&boundingBox={north:.4},{west:.4},{south:.4},{east:.4}&units=metric&format=json&includeStationName=true&includeStationLocation=true",
            self.cdo
        );
        let bytes = self.get_bytes(&url).await?;
        if bytes.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }
        Ok(parse_cdo(&bytes, lat, lon))
    }

    async fn load_meteostat(
        &self,
        lat: f64,
        lon: f64,
        time: &str,
    ) -> io::Result<(Vec<HistoryStation>, Vec<Sample>)> {
        let stations = self.meteostat_index().await?;
        let nearby = nearest_stations(&stations, lat, lon, NEAREST);
        let hour = parse_hour(time).ok_or_else(|| io::Error::other("bad Meteostat hour"))?;
        let year = hour.format("%Y").to_string();
        let stamp = hour.format("%Y-%m-%d %H:%M:%S").to_string();
        let mut out_stations = Vec::new();
        let mut samples = Vec::new();
        for station in nearby {
            let url = format!("{}/hourly/{}/{}.csv.gz", self.meteo, year, station.id);
            let Ok(bytes) = self.get_bytes(&url).await else {
                continue;
            };
            if let Some(sample) = parse_meteostat_hourly(&bytes, &stamp, station.lat, station.lon) {
                out_stations.push(HistoryStation {
                    id: station.id.clone(),
                    name: station.name.clone(),
                    lat: station.lat,
                    lon: station.lon,
                    distance_km: great_circle_km(lat, lon, station.lat, station.lon),
                });
                samples.push(sample);
            }
        }
        Ok((out_stations, samples))
    }

    async fn meteostat_index(&self) -> io::Result<Vec<MeteoStation>> {
        if let Some(cached) = read_station_cache() {
            return Ok(cached);
        }
        let bytes = self.get_bytes(&self.stations).await?;
        let stations = parse_meteostat_stations(&bytes)?;
        let _ = write_station_cache(&stations);
        Ok(stations)
    }

    async fn get_bytes(&self, url: &str) -> io::Result<Vec<u8>> {
        if let Some(path) = url.strip_prefix("file://") {
            return fs::read(path);
        }
        if url.starts_with('/') || PathBuf::from(url).exists() {
            return fs::read(url);
        }
        let response = self.http.get(url).send().await.map_err(io::Error::other)?;
        if !response.status().is_success() {
            return Err(io::Error::other(format!(
                "{url} returned {}",
                response.status()
            )));
        }
        let bytes = response.bytes().await.map_err(io::Error::other)?;
        if bytes.len() > MAX_BODY {
            return Err(io::Error::other("history body too large"));
        }
        Ok(bytes.to_vec())
    }
}

#[derive(Clone, Debug)]
struct MeteoStation {
    id: String,
    name: String,
    lat: f64,
    lon: f64,
}

#[derive(Deserialize)]
struct CdoRow {
    #[serde(default, rename = "STATION")]
    station: String,
    #[serde(default, rename = "NAME")]
    name: String,
    #[serde(default, rename = "LATITUDE", deserialize_with = "num_or_str")]
    lat: Option<f64>,
    #[serde(default, rename = "LONGITUDE", deserialize_with = "num_or_str")]
    lon: Option<f64>,
    #[serde(default, rename = "TAVG", deserialize_with = "num_or_str")]
    tavg: Option<f64>,
    #[serde(default, rename = "TMAX", deserialize_with = "num_or_str")]
    tmax: Option<f64>,
    #[serde(default, rename = "TMIN", deserialize_with = "num_or_str")]
    tmin: Option<f64>,
    #[serde(default, rename = "PRCP", deserialize_with = "num_or_str")]
    prcp: Option<f64>,
    #[serde(default, rename = "AWND", deserialize_with = "num_or_str")]
    awnd: Option<f64>,
}

fn num_or_str<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(match value {
        serde_json::Value::Null => None,
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.parse().ok(),
        _ => None,
    })
}

fn parse_cdo(bytes: &[u8], lat: f64, lon: f64) -> (Vec<HistoryStation>, Vec<Sample>) {
    let rows: Vec<CdoRow> = serde_json::from_slice(bytes).unwrap_or_default();
    let mut stations = Vec::new();
    let mut samples = Vec::new();
    for row in rows {
        let Some(plat) = row.lat else { continue };
        let Some(plon) = row.lon else { continue };
        let temp = row.tavg.or_else(|| match (row.tmax, row.tmin) {
            (Some(a), Some(b)) => Some((a + b) / 2.0),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            _ => None,
        });
        let mut sample = Sample {
            lat: plat,
            lon: plon,
            precip_mm: row.prcp,
            ..Sample::default()
        };
        sample.temp_c[0] = temp;
        if let Some(awnd) = row.awnd {
            sample.wind_kt[0] = Some(awnd * 1.943844);
        }
        stations.push(HistoryStation {
            id: row.station,
            name: row.name,
            lat: plat,
            lon: plon,
            distance_km: great_circle_km(lat, lon, plat, plon),
        });
        samples.push(sample);
    }
    (stations, samples)
}

fn parse_meteostat_stations(bytes: &[u8]) -> io::Result<Vec<MeteoStation>> {
    let text = inflate(bytes)?;
    let value: serde_json::Value = serde_json::from_slice(&text).map_err(io::Error::other)?;
    let items = value.as_array().cloned().unwrap_or_default();
    Ok(items
        .iter()
        .filter_map(|item| {
            let id = item.get("id")?.as_str()?.to_owned();
            let loc = item.get("location")?;
            let lat = loc.get("latitude")?.as_f64()?;
            let lon = loc.get("longitude")?.as_f64()?;
            let name = item
                .get("name")
                .and_then(|n| n.get("en").or_else(|| n.get("en-US")))
                .and_then(|v| v.as_str())
                .unwrap_or(&id)
                .to_owned();
            Some(MeteoStation { id, name, lat, lon })
        })
        .collect())
}

fn nearest_stations(stations: &[MeteoStation], lat: f64, lon: f64, n: usize) -> Vec<MeteoStation> {
    let mut ranked: Vec<_> = stations
        .iter()
        .map(|s| (great_circle_km(lat, lon, s.lat, s.lon), s.clone()))
        .filter(|(d, _)| *d <= 250.0)
        .collect();
    ranked.sort_by(|a, b| a.0.total_cmp(&b.0));
    ranked.into_iter().take(n).map(|(_, s)| s).collect()
}

fn parse_meteostat_hourly(bytes: &[u8], stamp: &str, lat: f64, lon: f64) -> Option<Sample> {
    let text = inflate(bytes).ok()?;
    let csv = String::from_utf8_lossy(&text);
    let mut lines = csv.lines();
    let header = lines.next()?;
    let cols: Vec<&str> = header.split(',').collect();
    let idx = |name: &str| cols.iter().position(|c| *c == name);
    let time_i = idx("time")?;
    for line in lines {
        let cells: Vec<&str> = line.split(',').collect();
        if cells.get(time_i).copied() != Some(stamp)
            && cells.get(time_i).map(|t| t.replace('T', " ")) != Some(stamp.to_owned())
        {
            continue;
        }
        let cell = |name: &str| {
            idx(name)
                .and_then(|i| cells.get(i))
                .and_then(|s| s.parse::<f64>().ok())
        };
        let mut sample = Sample {
            lat,
            lon,
            precip_mm: cell("prcp"),
            pressure_hpa: cell("pres"),
            ..Sample::default()
        };
        sample.temp_c[0] = cell("temp");
        sample.water_pct[0] = cell("rhum");
        if let Some(kmh) = cell("wspd") {
            sample.wind_kt[0] = Some(kmh / 1.852);
        }
        sample.wind_dir[0] = cell("wdir");
        return Some(sample);
    }
    None
}

fn inflate(bytes: &[u8]) -> io::Result<Vec<u8>> {
    if bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
        let mut out = Vec::new();
        GzDecoder::new(bytes).read_to_end(&mut out)?;
        return Ok(out);
    }
    Ok(bytes.to_vec())
}

fn cache_path() -> Option<PathBuf> {
    let base = env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join("omastorm").join("meteostat").join("lite.json"))
}

fn read_station_cache() -> Option<Vec<MeteoStation>> {
    let path = cache_path()?;
    let bytes = fs::read(path).ok()?;
    parse_meteostat_stations(&bytes).ok()
}

fn write_station_cache(stations: &[MeteoStation]) -> io::Result<()> {
    let path = cache_path().ok_or_else(|| io::Error::other("no cache dir"))?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let value: Vec<serde_json::Value> = stations
        .iter()
        .map(|s| {
            serde_json::json!({
                "id": s.id,
                "name": {"en": s.name},
                "location": {"latitude": s.lat, "longitude": s.lon}
            })
        })
        .collect();
    fs::write(path, serde_json::to_vec(&value)?)
}

fn great_circle_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let (dp, dl) = ((lat2 - lat1).to_radians(), (lon2 - lon1).to_radians());
    let h = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * 6371.0 * h.clamp(0.0, 1.0).sqrt().asin()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cdo_steps_whole_days_and_meteostat_steps_hours() {
        assert_eq!(step_time("cdo", "2020-01-15", 1).unwrap(), "2020-01-16");
        assert_eq!(
            step_time("meteostat", "2020-01-15T12:00:00Z", -2).unwrap(),
            "2020-01-15T10:00:00Z"
        );
        assert_eq!(step_of("cdo"), "day");
        assert_eq!(step_of("meteostat"), "hour");
    }

    #[test]
    fn cdo_json_builds_santiago_temperature_samples() {
        let body = br#"[{"STATION":"CI000085533","NAME":"PUDAHUEL","LATITUDE":-33.39,"LONGITUDE":-70.79,"TAVG":18.0,"TMAX":24.0,"TMIN":12.0,"PRCP":0.0,"AWND":2.0}]"#;
        let (stations, samples) = parse_cdo(body, -33.45, -70.67);
        assert_eq!(stations[0].id, "CI000085533");
        assert!((samples[0].temp_c[0].unwrap() - 18.0).abs() < 0.01);
        assert_eq!(samples[0].precip_mm, Some(0.0));
        assert!(samples[0].wind_kt[0].unwrap() > 3.0);
    }

    #[test]
    fn meteostat_hourly_csv_picks_the_requested_hour() {
        let csv = b"time,temp,rhum,prcp,wdir,wspd,pres\n2020-01-15 11:00:00,10,40,0,180,10,1012\n2020-01-15 12:00:00,18,55,1.2,270,20,1016\n";
        let sample = parse_meteostat_hourly(csv, "2020-01-15 12:00:00", -33.4, -70.7).unwrap();
        assert!((sample.temp_c[0].unwrap() - 18.0).abs() < 0.01);
        assert_eq!(sample.precip_mm, Some(1.2));
        assert!((sample.wind_kt[0].unwrap() - 20.0 / 1.852).abs() < 0.05);
        assert_eq!(sample.wind_dir[0], Some(270.0));
    }

    #[test]
    fn meteostat_lite_json_keeps_coordinates() {
        let body = br#"[{"id":"85574","name":{"en":"Pudahuel"},"location":{"latitude":-33.39,"longitude":-70.79}}]"#;
        let stations = parse_meteostat_stations(body).unwrap();
        assert_eq!(stations[0].id, "85574");
        let near = nearest_stations(&stations, -33.45, -70.67, 3);
        assert_eq!(near.len(), 1);
    }
}

//! Dirección Meteorológica de Chile (www.meteochile.gob.cl).
//!
//! Observations: DMC METAR/TAF are identified by ICAO and read through AWC
//! `metar?ids=` so local aerodromes such as SCTB appear even when the AWC
//! bbox only returns SCEL. WRF-DMC (GFS- and ECMWF-driven regional runs)
//! uses the Servicios Climáticos `getDatosModelo` service when
//! `OMASTORM_METEOCHILE_USER` and `OMASTORM_METEOCHILE_TOKEN` are set.
//! Live mode only.

use crate::airports;
use crate::fields::{self, Sample};
use crate::protocol::Frame;
use serde::Deserialize;
use std::{env, io};

const AWC_URL: &str = "https://aviationweather.gov/api/data";
const DMC_MODELO: &str =
    "https://climatologia.meteochile.gob.cl/application/serviciosb/getDatosModelo";
const USER_AGENT: &str = concat!(
    "omastorm/",
    env!("CARGO_PKG_VERSION"),
    " (https://omastorm.com)"
);
const FETCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
const MAX_BODY: usize = 4 << 20;
const ATTRIBUTION: &str = "Dirección Meteorológica de Chile";

fn meteochile_creds() -> (String, String) {
    let user = env::var("OMASTORM_METEOCHILE_USER").unwrap_or_default();
    let token = env::var("OMASTORM_METEOCHILE_TOKEN").unwrap_or_default();
    if !user.is_empty() && !token.is_empty() {
        return (user, token);
    }
    let path = env::var("OMASTORM_SOURCES")
        .ok()
        .map(std::path::PathBuf::from)
        .or_else(|| {
            env::var("XDG_CONFIG_HOME")
                .ok()
                .map(|h| std::path::PathBuf::from(h).join("omastorm/sources.json"))
        })
        .or_else(|| {
            env::var("HOME")
                .ok()
                .map(|h| std::path::PathBuf::from(h).join(".config/omastorm/sources.json"))
        });
    let Some(path) = path else {
        return (user, token);
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return (user, token);
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return (user, token);
    };
    (
        value
            .get("meteochileUser")
            .and_then(|v| v.as_str())
            .unwrap_or(&user)
            .to_owned(),
        value
            .get("meteochileToken")
            .and_then(|v| v.as_str())
            .unwrap_or(&token)
            .to_owned(),
    )
}

pub fn is_dmc(id: &str) -> bool {
    matches!(id, "dmc" | "dmc_wrf_gfs" | "dmc_wrf_ecmwf")
}

pub struct Client {
    awc: String,
    modelo: String,
    http: reqwest::Client,
}

impl Client {
    pub fn open() -> io::Result<Client> {
        let awc = env::var("OMASTORM_AVIATION_URL").unwrap_or_else(|_| AWC_URL.into());
        let modelo =
            env::var("OMASTORM_METEOCHILE_MODELO_URL").unwrap_or_else(|_| DMC_MODELO.into());
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(FETCH_TIMEOUT)
            .build()
            .map_err(io::Error::other)?;
        Ok(Client {
            awc: awc.trim_end_matches('/').to_owned(),
            modelo: modelo.trim_end_matches('/').to_owned(),
            http,
        })
    }

    pub async fn fetch(
        &self,
        lat: f64,
        lon: f64,
        source: &str,
        product: &str,
        altitude: u32,
    ) -> io::Result<(Frame, Vec<u8>, Vec<u8>)> {
        if !airports::in_chile(lat, lon) {
            return Err(io::Error::other(
                "MeteoChile covers Chile (and Isla de Pascua).",
            ));
        }
        let spec =
            fields::altitude(altitude).ok_or_else(|| io::Error::other("unknown altitude"))?;
        let samples = if source == "dmc" {
            self.load_reports(lat, lon).await?
        } else {
            self.load_wrf(lat, lon, source).await?
        };
        let product = if fields::is_field(product) {
            product
        } else {
            "TEMP"
        };
        let time = chrono::Utc::now().format("%Y-%m-%dT%H:00:00Z").to_string();
        fields::raster(&samples, lat, lon, source, product, spec, &time)
    }

    async fn load_reports(&self, lat: f64, lon: f64) -> io::Result<Vec<Sample>> {
        let mut nearest = airports::CHILE.to_vec();
        nearest.sort_by(|a, b| {
            airports::great_circle_km(lat, lon, a.lat, a.lon)
                .total_cmp(&airports::great_circle_km(lat, lon, b.lat, b.lon))
        });
        let ids = nearest
            .iter()
            .take(8)
            .map(|a| a.icao)
            .collect::<Vec<_>>()
            .join(",");
        let url = format!("{}/metar?ids={ids}&format=json", self.awc);
        let body = self.get_json(&url).await?;
        Ok(metars_to_samples(&body))
    }

    async fn load_wrf(&self, lat: f64, lon: f64, source: &str) -> io::Result<Vec<Sample>> {
        let (user, token) = meteochile_creds();
        if user.is_empty() || token.is_empty() {
            return Err(io::Error::other(
                "MeteoChile WRF-DMC needs credentials in API SOURCES or OMASTORM_METEOCHILE_USER / OMASTORM_METEOCHILE_TOKEN.",
            ));
        }
        let (airport, _) = airports::nearest(lat, lon)
            .ok_or_else(|| io::Error::other("no MeteoChile station near the view"))?;
        if airport.dmc_id.is_empty() {
            return Err(io::Error::other(format!(
                "no DMC station id for {}",
                airport.icao
            )));
        }
        let url = format!(
            "{}/{}?usuario={}&token={}",
            self.modelo, airport.dmc_id, user, token
        );
        let body = self.get_json(&url).await?;
        let driver = if source.ends_with("ecmwf") {
            "ECMWF"
        } else {
            "GFS"
        };
        let sample = wrf_sample(&body, driver, airport.lat, airport.lon).ok_or_else(|| {
            io::Error::other(format!(
                "MeteoChile WRF-DMC ({driver}) had no usable values at {}",
                airport.icao
            ))
        })?;
        Ok(vec![sample])
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
            return Err(io::Error::other("MeteoChile body too large"));
        }
        serde_json::from_slice(&bytes).map_err(io::Error::other)
    }
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
    temp: Option<f64>,
    #[serde(default)]
    wspd: Option<f64>,
    #[serde(default)]
    wdir: Option<f64>,
    #[serde(default)]
    altim: Option<f64>,
}

fn metars_to_samples(value: &serde_json::Value) -> Vec<Sample> {
    let items = match value {
        serde_json::Value::Array(items) => items,
        _ => return Vec::new(),
    };
    items
        .iter()
        .filter_map(|item| serde_json::from_value::<MetarRecord>(item.clone()).ok())
        .filter(|m| !m.icao_id.is_empty())
        .map(|m| {
            let temp = m.temp.or_else(|| parse_temp(&m.raw_ob));
            let wind = m.wspd;
            let dir = m.wdir;
            Sample {
                lat: m.lat,
                lon: m.lon,
                wind_kt: [wind, None, None, None, None, None],
                wind_dir: [dir, None, None, None, None, None],
                pressure_hpa: m.altim,
                height_m: [None; 6],
                water_pct: [None; 6],
                temp_c: [temp, None, None, None, None, None],
                precip_mm: None,
            }
        })
        .filter(|s| s.lat.abs() <= 90.0)
        .collect()
}

fn parse_temp(raw: &str) -> Option<f64> {
    raw.split_whitespace().find_map(|token| {
        let (a, b) = token.split_once('/')?;
        if a.len() > 3 || b.is_empty() {
            return None;
        }
        awc_temp(a)
    })
}

fn awc_temp(text: &str) -> Option<f64> {
    let negative = text.starts_with('M');
    let digits = text.trim_start_matches('M');
    let value = digits.parse::<f64>().ok()?;
    Some(if negative { -value } else { value })
}

fn wrf_sample(value: &serde_json::Value, driver: &str, lat: f64, lon: f64) -> Option<Sample> {
    let series = value.pointer("/datosEstacion/datos").or_else(|| {
        value
            .get("datosEstaciones")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .and_then(|v| v.get("datos"))
    });
    let rows = series.and_then(|v| v.as_array())?;
    let mut temp = None;
    let mut wind = None;
    let mut dir = None;
    let mut rh = None;
    let mut precip = None;
    for row in rows.iter().rev() {
        let fuente = row
            .get("fuente")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_ascii_uppercase();
        if !fuente.is_empty() && !fuente.contains(driver) {
            continue;
        }
        let name = row
            .get("nombre")
            .or_else(|| row.get("variable"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let valor = row
            .get("valor")
            .or_else(|| row.get("valorPronosticado"))
            .and_then(json_f64)
            .or_else(|| {
                row.get("valores")
                    .and_then(|v| v.as_array())
                    .and_then(|a| a.last())
                    .and_then(json_f64)
            });
        let Some(v) = valor else { continue };
        if name.contains("temp") && temp.is_none() {
            temp = Some(v);
        } else if (name.contains("viento") || name.contains("wind"))
            && !name.contains("dir")
            && wind.is_none()
        {
            wind = Some(v);
        } else if name.contains("dir") && dir.is_none() {
            dir = Some(v);
        } else if (name.contains("humedad") || name.contains("rh")) && rh.is_none() {
            rh = Some(v);
        } else if name.contains("agua") || name.contains("precip") {
            precip = Some(v);
        }
    }
    if temp.is_none() && wind.is_none() {
        return None;
    }
    let _ = ATTRIBUTION;
    Some(Sample {
        lat,
        lon,
        wind_kt: [wind, None, None, None, None, None],
        wind_dir: [dir, None, None, None, None, None],
        pressure_hpa: None,
        height_m: [None; 6],
        water_pct: [rh, None, None, None, None, None],
        temp_c: [temp, None, None, None, None, None],
        precip_mm: precip,
    })
}

fn json_f64(value: &serde_json::Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_i64().map(|n| n as f64))
        .or_else(|| {
            value.as_str().and_then(|s| {
                s.split_whitespace()
                    .next()
                    .and_then(|n| n.replace(',', ".").parse().ok())
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metar_json_keeps_sctb_temperature() {
        let body = serde_json::json!([{
            "icaoId": "SCTB",
            "lat": -33.456,
            "lon": -70.547,
            "rawOb": "METAR SCTB 112100Z 25006KT 9999 BKN100 15/05 Q1019",
            "temp": 15.0,
            "wspd": 6.0,
            "wdir": 250.0,
            "altim": 1019.0
        }]);
        let samples = metars_to_samples(&body);
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].temp_c[0], Some(15.0));
        assert_eq!(samples[0].wind_kt[0], Some(6.0));
    }

    #[test]
    fn raw_metar_temperature_parses_minus() {
        assert_eq!(
            parse_temp("SCTB 000000Z 00000KT CAVOK M03/M05 Q1020"),
            Some(-3.0)
        );
    }
}

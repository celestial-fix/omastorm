//! Current conditions from a user-chosen source (DESIGN.md: no forecasts).
//! The UI sends `set_weather` with the source and the user's key or WeeWX
//! URL; the key never enters `state`. Network happens here, not in the UI.

use crate::protocol::{Weather, WeatherStatus};
use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;
use std::{io, time::Duration};

const USER_AGENT: &str = concat!(
    "omastorm/",
    env!("CARGO_PKG_VERSION"),
    " (https://omastorm.com)"
);
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_BODY: usize = 256 * 1024;
const STALE_AFTER_SECS: i64 = 30 * 60;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    Weewx,
    WeatherApi,
    OpenWeatherMap,
    Tomorrow,
    VisualCrossing,
}

impl Source {
    pub fn parse(name: &str) -> Option<Self> {
        let compact: String = name
            .trim()
            .chars()
            .filter(|c| !c.is_ascii_whitespace() && *c != '_' && *c != '-' && *c != '.')
            .map(|c| c.to_ascii_lowercase())
            .collect();
        match compact.as_str() {
            "" | "off" | "none" => None,
            "weewx" => Some(Self::Weewx),
            "weatherapi" => Some(Self::WeatherApi),
            "openweathermap" | "owm" | "openweather" => Some(Self::OpenWeatherMap),
            "tomorrow" | "tomorrowio" => Some(Self::Tomorrow),
            "visualcrossing" => Some(Self::VisualCrossing),
            _ => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Weewx => "weewx",
            Self::WeatherApi => "weatherapi",
            Self::OpenWeatherMap => "openweathermap",
            Self::Tomorrow => "tomorrow",
            Self::VisualCrossing => "visualcrossing",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Weewx => "WeeWX",
            Self::WeatherApi => "WeatherAPI",
            Self::OpenWeatherMap => "OpenWeatherMap",
            Self::Tomorrow => "Tomorrow.io",
            Self::VisualCrossing => "Visual Crossing",
        }
    }

    fn attribution(self) -> &'static str {
        self.name()
    }

    fn poll_every(self) -> Duration {
        match self {
            Self::Weewx => Duration::from_secs(60),
            _ => Duration::from_secs(600),
        }
    }

    fn needs_key(self) -> bool {
        !matches!(self, Self::Weewx)
    }

    fn needs_point(self) -> bool {
        !matches!(self, Self::Weewx)
    }
}

#[derive(Clone, PartialEq)]
pub struct Request {
    pub source: Source,
    pub api_key: String,
    pub url: String,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
}

impl Request {
    pub fn poll_every(&self) -> Duration {
        self.source.poll_every()
    }
}

pub struct FetchError {
    pub status: WeatherStatus,
    pub message: String,
}

pub fn client() -> io::Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(FETCH_TIMEOUT)
        .build()
        .map_err(io::Error::other)
}

/// Validate a `set_weather` command. An empty source clears the feed.
pub fn parse_request(
    source: &str,
    api_key: &str,
    url: &str,
    lat: Option<f64>,
    lon: Option<f64>,
) -> Result<Option<Request>, String> {
    let source = source.trim();
    if source.is_empty() {
        return Ok(None);
    }
    let Some(source) = Source::parse(source) else {
        return Err(
            "set_weather source must be weewx, weatherapi, openweathermap, tomorrow, or visualcrossing."
                .into(),
        );
    };
    if lat.is_some_and(|lat| !(-90.0..=90.0).contains(&lat)) {
        return Err("set_weather needs lat in [-90, 90].".into());
    }
    if lon.is_some_and(|lon| !(-180.0..=180.0).contains(&lon)) {
        return Err("set_weather needs lon in [-180, 180].".into());
    }
    if source.needs_point() && (lat.is_none() || lon.is_none()) {
        return Err("set_weather needs lat and lon for that source.".into());
    }
    let api_key = api_key.trim().to_owned();
    let url = url.trim().to_owned();
    if source.needs_key() && api_key.is_empty() {
        return Err(format!("set_weather needs apiKey for {}.", source.name()));
    }
    if source == Source::Weewx {
        if url.is_empty() {
            return Err("set_weather needs url for WeeWX.".into());
        }
        let parsed = reqwest::Url::parse(&url).map_err(|_| "set_weather url is not a URL.")?;
        if parsed.scheme() != "http" && parsed.scheme() != "https" {
            return Err("set_weather url must be http or https.".into());
        }
        if parsed.host_str().is_none() {
            return Err("set_weather url needs a host.".into());
        }
    }
    Ok(Some(Request {
        source,
        api_key,
        url,
        lat,
        lon,
    }))
}

pub fn loading(request: &Request) -> Weather {
    Weather {
        source: request.source.id().into(),
        source_name: request.source.name().into(),
        status: WeatherStatus::Loading,
        observed_at: String::new(),
        name: String::new(),
        temperature_c: None,
        condition: String::new(),
        wind_kmh: None,
        humidity: None,
        attribution: request.source.attribution().into(),
        message: String::new(),
    }
}

pub async fn fetch(client: &reqwest::Client, request: &Request) -> Result<Weather, FetchError> {
    let url = endpoint(request).map_err(|message| FetchError {
        status: WeatherStatus::Unavailable,
        message,
    })?;
    let bytes = get(client, &url).await?;
    let text = String::from_utf8_lossy(&bytes);
    let json: Value = serde_json::from_str(&text).map_err(|_| FetchError {
        status: WeatherStatus::Unavailable,
        message: format!("{} did not return JSON.", request.source.name()),
    })?;
    let mut weather = match request.source {
        Source::Weewx => parse_weewx(&json),
        Source::WeatherApi => parse_weatherapi(&json),
        Source::OpenWeatherMap => parse_openweathermap(&json),
        Source::Tomorrow => parse_tomorrow(&json),
        Source::VisualCrossing => parse_visualcrossing(&json),
    }
    .ok_or_else(|| FetchError {
        status: WeatherStatus::Unavailable,
        message: format!(
            "{} response had no current observation.",
            request.source.name()
        ),
    })?;
    weather.source = request.source.id().into();
    weather.source_name = request.source.name().into();
    weather.attribution = request.source.attribution().into();
    weather.status = if observation_age_secs(&weather.observed_at) >= STALE_AFTER_SECS {
        WeatherStatus::Stale
    } else {
        WeatherStatus::Ok
    };
    Ok(weather)
}

fn endpoint(request: &Request) -> Result<String, String> {
    match request.source {
        Source::Weewx => Ok(request.url.clone()),
        Source::WeatherApi => {
            let (lat, lon) = point(request)?;
            Ok(format!(
                "https://api.weatherapi.com/v1/current.json?key={}&q={lat},{lon}",
                enc(&request.api_key)
            ))
        }
        Source::OpenWeatherMap => {
            let (lat, lon) = point(request)?;
            Ok(format!(
                "https://api.openweathermap.org/data/2.5/weather?lat={lat}&lon={lon}&units=metric&appid={}",
                enc(&request.api_key)
            ))
        }
        Source::Tomorrow => {
            let (lat, lon) = point(request)?;
            Ok(format!(
                "https://api.tomorrow.io/v4/weather/realtime?location={lat},{lon}&apikey={}",
                enc(&request.api_key)
            ))
        }
        Source::VisualCrossing => {
            let (lat, lon) = point(request)?;
            Ok(format!(
                "https://weather.visualcrossing.com/VisualCrossingWebServices/rest/services/timeline/{lat},{lon}/today?unitGroup=metric&include=current&contentType=json&key={}",
                enc(&request.api_key)
            ))
        }
    }
}

fn point(request: &Request) -> Result<(f64, f64), String> {
    match (request.lat, request.lon) {
        (Some(lat), Some(lon)) => Ok((lat, lon)),
        _ => Err("lat and lon are required".into()),
    }
}

fn enc(value: &str) -> String {
    let mut out = String::new();
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

async fn get(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, FetchError> {
    let mut response = client.get(url).send().await.map_err(|_| FetchError {
        status: WeatherStatus::Offline,
        message: "Weather source unreachable.".into(),
    })?;
    let status = response.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(FetchError {
            status: WeatherStatus::Unavailable,
            message: "Weather source rejected the API key.".into(),
        });
    }
    if !status.is_success() {
        return Err(FetchError {
            status: WeatherStatus::Unavailable,
            message: format!("Weather source returned HTTP {status}."),
        });
    }
    if response
        .content_length()
        .is_some_and(|n| n > MAX_BODY as u64)
    {
        return Err(FetchError {
            status: WeatherStatus::Unavailable,
            message: "Weather source body over the size limit.".into(),
        });
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| FetchError {
        status: WeatherStatus::Offline,
        message: "Weather source unread.".into(),
    })? {
        if bytes.len() + chunk.len() > MAX_BODY {
            return Err(FetchError {
                status: WeatherStatus::Unavailable,
                message: "Weather source body over the size limit.".into(),
            });
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn parse_openweathermap(json: &Value) -> Option<Weather> {
    let weather0 = json.get("weather")?.as_array()?.first();
    let condition = weather0
        .and_then(|w| w.get("description"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    Some(obs(
        json.get("name").and_then(Value::as_str).unwrap_or(""),
        number(json.pointer("/main/temp")),
        condition,
        number(json.pointer("/wind/speed")).map(|ms| ms * 3.6),
        number(json.pointer("/main/humidity")),
        unix_or_iso(json.get("dt")),
    ))
}

fn parse_weatherapi(json: &Value) -> Option<Weather> {
    let current = json.get("current")?;
    current.get("temp_c")?;
    let name = json
        .pointer("/location/name")
        .and_then(Value::as_str)
        .unwrap_or("");
    Some(obs(
        name,
        number(current.get("temp_c")),
        current
            .pointer("/condition/text")
            .and_then(Value::as_str)
            .unwrap_or(""),
        number(current.get("wind_kph")),
        number(current.get("humidity")),
        unix_or_iso(current.get("last_updated_epoch"))
            .or_else(|| iso_string(current.get("last_updated"))),
    ))
}

fn parse_tomorrow(json: &Value) -> Option<Weather> {
    let values = json.pointer("/data/values")?;
    let temp = number(values.get("temperature"))?;
    let name = json
        .pointer("/location/name")
        .and_then(Value::as_str)
        .unwrap_or("");
    let code = values.get("weatherCode").and_then(Value::as_i64);
    Some(obs(
        name,
        Some(temp),
        tomorrow_condition(code),
        number(values.get("windSpeed")).map(|ms| ms * 3.6),
        number(values.get("humidity")),
        iso_string(json.pointer("/data/time")),
    ))
}

fn parse_visualcrossing(json: &Value) -> Option<Weather> {
    let current = json.get("currentConditions")?;
    let name = json
        .get("resolvedAddress")
        .and_then(Value::as_str)
        .or_else(|| json.get("address").and_then(Value::as_str))
        .unwrap_or("");
    Some(obs(
        name,
        number(current.get("temp")),
        current
            .get("conditions")
            .and_then(Value::as_str)
            .unwrap_or(""),
        number(current.get("windspeed")),
        number(current.get("humidity")),
        unix_or_iso(current.get("datetimeEpoch")).or_else(|| iso_string(current.get("datetime"))),
    ))
}

fn parse_weewx(json: &Value) -> Option<Weather> {
    let holders = [
        Some(json),
        json.get("current"),
        json.get("stats").and_then(|s| s.get("current")),
        json.get("loop"),
    ];
    let mut temperature = None;
    let mut humidity = None;
    let mut wind = None;
    let mut when = String::new();
    let mut name = String::new();
    for holder in holders.into_iter().flatten() {
        if temperature.is_none() {
            temperature = number(holder.get("outTemp_C"))
                .or_else(|| number(holder.get("temperature_C")))
                .or_else(|| number(holder.get("temp_C")))
                .or_else(|| number(holder.get("outTemp")))
                .or_else(|| fahrenheit(holder.get("outTemp_F")));
        }
        if humidity.is_none() {
            humidity = number(holder.get("outHumidity")).or_else(|| number(holder.get("humidity")));
        }
        if wind.is_none() {
            wind = number(holder.get("windSpeed_kph"))
                .or_else(|| number(holder.get("windSpeed_kmh")))
                .or_else(|| {
                    number(holder.get("windSpeed_mps"))
                        .or_else(|| number(holder.get("windSpeed")))
                        .map(|ms| ms * 3.6)
                });
        }
        if when.is_empty() {
            when = unix_or_iso(holder.get("dateTime"))
                .or_else(|| unix_or_iso(holder.get("datetime")))
                .unwrap_or_default();
        }
        if name.is_empty() {
            name = holder
                .get("location")
                .and_then(Value::as_str)
                .or_else(|| holder.get("station").and_then(Value::as_str))
                .unwrap_or("")
                .to_owned();
        }
    }
    if name.is_empty() {
        name = json
            .pointer("/station/location")
            .and_then(Value::as_str)
            .or_else(|| {
                json.pointer("/stationInfo/location")
                    .and_then(Value::as_str)
            })
            .unwrap_or("")
            .to_owned();
    }
    temperature?;
    Some(obs(
        &name,
        temperature,
        "",
        wind,
        humidity,
        Some(when).filter(|s| !s.is_empty()),
    ))
}

fn tomorrow_condition(code: Option<i64>) -> String {
    match code {
        Some(1000) => "Clear",
        Some(1100) => "Mostly clear",
        Some(1101) => "Partly cloudy",
        Some(1102) => "Mostly cloudy",
        Some(1001) => "Cloudy",
        Some(2000 | 2100) => "Fog",
        Some(4000) => "Drizzle",
        Some(4001) => "Rain",
        Some(4200) => "Light rain",
        Some(4201) => "Heavy rain",
        Some(5000 | 5101) => "Snow",
        Some(5001 | 5100) => "Light snow",
        Some(8000) => "Thunderstorm",
        Some(n) => return format!("Code {n}"),
        None => "",
    }
    .into()
}

fn obs(
    name: &str,
    temperature_c: Option<f64>,
    condition: impl Into<String>,
    wind_kmh: Option<f64>,
    humidity: Option<f64>,
    observed_at: Option<String>,
) -> Weather {
    Weather {
        source: String::new(),
        source_name: String::new(),
        status: WeatherStatus::Ok,
        observed_at: observed_at.unwrap_or_default(),
        name: name.trim().to_owned(),
        temperature_c,
        condition: condition.into(),
        wind_kmh,
        humidity,
        attribution: String::new(),
        message: String::new(),
    }
}

fn number(value: Option<&Value>) -> Option<f64> {
    let value = value?;
    if let Some(n) = value.as_f64() {
        return is_finite(n);
    }
    if let Some(s) = value.as_str() {
        let trimmed = s
            .split(|c: char| c == '°' || c.is_ascii_alphabetic() || c == ' ')
            .next()
            .unwrap_or(s)
            .trim();
        return trimmed.parse().ok().and_then(is_finite);
    }
    None
}

fn is_finite(n: f64) -> Option<f64> {
    n.is_finite().then_some(n)
}

fn fahrenheit(value: Option<&Value>) -> Option<f64> {
    number(value).map(|f| (f - 32.0) * 5.0 / 9.0)
}

fn unix_or_iso(value: Option<&Value>) -> Option<String> {
    let value = value?;
    if let Some(n) = value.as_i64().or_else(|| value.as_f64().map(|n| n as i64)) {
        let secs = if n > 1_000_000_000_000 { n / 1000 } else { n };
        return Utc.timestamp_opt(secs, 0).single().map(|t| t.to_rfc3339());
    }
    iso_string(Some(value))
}

fn iso_string(value: Option<&Value>) -> Option<String> {
    let text = value?.as_str()?.trim();
    if text.is_empty() {
        return None;
    }
    if let Ok(t) = DateTime::parse_from_rfc3339(text) {
        return Some(t.with_timezone(&Utc).to_rfc3339());
    }
    Some(text.to_owned())
}

fn observation_age_secs(observed_at: &str) -> i64 {
    let Ok(when) = DateTime::parse_from_rfc3339(observed_at) else {
        return 0;
    };
    (Utc::now() - when.with_timezone(&Utc)).num_seconds()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources_and_validation() {
        assert_eq!(
            Source::parse("OpenWeatherMap"),
            Some(Source::OpenWeatherMap)
        );
        assert_eq!(Source::parse("tomorrow.io"), Some(Source::Tomorrow));
        assert_eq!(
            Source::parse("visual-crossing"),
            Some(Source::VisualCrossing)
        );
        assert!(Source::parse("nws").is_none());
        assert!(parse_request("", "", "", None, None).unwrap().is_none());
        assert!(parse_request("openweathermap", "", "", Some(35.0), Some(-97.0)).is_err());
        assert!(parse_request("openweathermap", "k", "", None, None).is_err());
        assert!(parse_request("weewx", "", "", None, None).is_err());
        assert!(parse_request("weewx", "", "ftp://x", None, None).is_err());
        let req = parse_request("weewx", "", "http://192.168.1.8:8080/data.json", None, None)
            .unwrap()
            .unwrap();
        assert_eq!(req.source, Source::Weewx);
        let owm = parse_request("owm", "secret", "", Some(36.2), Some(-80.0))
            .unwrap()
            .unwrap();
        assert_eq!(owm.source, Source::OpenWeatherMap);
        assert!(endpoint(&owm).unwrap().contains("appid=secret"));
        assert!(!endpoint(&owm).unwrap().contains("forecast"));
    }

    #[test]
    fn openweathermap_current() {
        let json = serde_json::from_str(
            r#"{"coord":{"lon":-79.98,"lat":36.24},"weather":[{"description":"overcast clouds"}],
               "main":{"temp":22.1,"humidity":64},"wind":{"speed":3.0},"dt":1694371200,"name":"Stokesdale"}"#,
        )
        .unwrap();
        let w = parse_openweathermap(&json).unwrap();
        assert_eq!(w.name, "Stokesdale");
        assert!((w.temperature_c.unwrap() - 22.1).abs() < 1e-9);
        assert_eq!(w.condition, "overcast clouds");
        assert!((w.wind_kmh.unwrap() - 10.8).abs() < 1e-9);
        assert_eq!(w.humidity.unwrap(), 64.0);
        assert!(w.observed_at.contains("2023-09-10"));
    }

    #[test]
    fn weatherapi_current() {
        let json = serde_json::from_str(
            r#"{"location":{"name":"Stokesdale","lat":36.24,"lon":-79.98},
               "current":{"temp_c":21.0,"humidity":70,"wind_kph":12.0,
               "last_updated_epoch":1694371200,"condition":{"text":"Cloudy"}}}"#,
        )
        .unwrap();
        let w = parse_weatherapi(&json).unwrap();
        assert_eq!(w.name, "Stokesdale");
        assert_eq!(w.temperature_c.unwrap(), 21.0);
        assert_eq!(w.condition, "Cloudy");
        assert_eq!(w.wind_kmh.unwrap(), 12.0);
    }

    #[test]
    fn tomorrow_realtime() {
        let json = serde_json::from_str(
            r#"{"data":{"time":"2023-09-10T16:00:00Z","values":{"temperature":18.5,
               "humidity":55,"windSpeed":2.5,"weatherCode":1101}},
               "location":{"lat":36.24,"lon":-79.98,"name":"Stokesdale"}}"#,
        )
        .unwrap();
        let w = parse_tomorrow(&json).unwrap();
        assert_eq!(w.condition, "Partly cloudy");
        assert!((w.temperature_c.unwrap() - 18.5).abs() < 1e-9);
        assert!((w.wind_kmh.unwrap() - 9.0).abs() < 1e-9);
    }

    #[test]
    fn visualcrossing_current() {
        let json = serde_json::from_str(
            r#"{"resolvedAddress":"Stokesdale, NC, United States","latitude":36.237,"longitude":-79.979,
               "currentConditions":{"temp":19.0,"humidity":60,"windspeed":8.0,
               "conditions":"Overcast","datetimeEpoch":1694371200}}"#,
        )
        .unwrap();
        let w = parse_visualcrossing(&json).unwrap();
        assert!(w.name.contains("Stokesdale"));
        assert_eq!(w.temperature_c.unwrap(), 19.0);
        assert_eq!(w.condition, "Overcast");
    }

    #[test]
    fn weewx_station() {
        let json = serde_json::from_str(
            r#"{"station":{"location":"Back yard"},"outTemp_C":17.2,"outHumidity":48,
               "windSpeed_kph":6.0,"dateTime":1694371200}"#,
        )
        .unwrap();
        let w = parse_weewx(&json).unwrap();
        assert_eq!(w.name, "Back yard");
        assert!((w.temperature_c.unwrap() - 17.2).abs() < 1e-9);
        assert_eq!(w.humidity.unwrap(), 48.0);
        assert_eq!(w.wind_kmh.unwrap(), 6.0);
    }
}

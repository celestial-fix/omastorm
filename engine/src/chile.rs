//! Chile IFIS briefing: METAR, TAF, NOTAM, and SIGMET from
//! `aipchile.dgac.gob.cl` (DGAC). Chilean ICAO ids (`SC*`) and a view
//! over Chile never take NOAA AWC as the bulletin source.
//! `OMASTORM_CHILE_URL` overrides the site root.

use crate::protocol::{AviationStation, Bulletin, Hazard, Point};
use chrono::Datelike;
use std::io;

pub const ATTRIBUTION: &str = "DGAC Chile IFIS";
pub const DEFAULT_URL: &str = "https://aipchile.dgac.gob.cl";
const MAX_BODY: usize = 2 << 20;
const MAX_NOTAMS: usize = 12;
const MAX_STATIONS: usize = 32;
const NEAR_KM: f64 = 500.0;

/// Public Chilean aerodromes (AIP Chile / DGAC designators) with
/// aerodrome reference-point coordinates, so the map can draw ICAO
/// markers without asking NOAA where Chile is.
const AERODROMES: &[(&str, f64, f64)] = &[
    ("SCAR", -18.3483, -70.3389),  // Arica
    ("SCDA", -20.5353, -70.1814),  // Iquique
    ("SCCF", -22.4983, -68.9036),  // Calama
    ("SCFA", -23.4445, -70.4450),  // Antofagasta
    ("SCAT", -27.2611, -70.7792),  // Copiapó
    ("SCES", -26.3111, -69.7653),  // El Salvador
    ("SCIP", -27.1647, -109.4217), // Mataveri, Rapa Nui
    ("SCSE", -29.9161, -71.1994),  // La Serena
    ("SCLL", -28.5964, -70.7619),  // Vallenar
    ("SCEL", -33.3931, -70.7858),  // Santiago AMB
    ("SCVQ", -32.7456, -70.5986),  // Viña / Rodelillo area
    ("SCVM", -32.9497, -71.4786),  // Viña del Mar
    ("SCSN", -33.6561, -71.6142),  // Santo Domingo
    ("SCIR", -33.6650, -78.9294),  // Robinson Crusoe
    ("SCIC", -34.9667, -71.2167),  // Curicó
    ("SCCH", -36.5825, -72.0314),  // Chillán
    ("SCIE", -36.7725, -73.0631),  // Concepción
    ("SCGE", -37.4017, -72.4256),  // Los Ángeles
    ("SCQP", -38.9259, -72.6515),  // Temuco Araucanía
    ("SCTC", -38.7670, -72.6370),  // Temuco Maquehue
    ("SCVD", -39.6500, -73.0861),  // Valdivia
    ("SCJO", -40.6111, -73.0611),  // Osorno
    ("SCTE", -41.4389, -73.0939),  // Puerto Montt
    ("SCPQ", -42.3403, -73.7156),  // Castro
    ("SCBA", -45.9161, -71.6894),  // Balmaceda
    ("SCCY", -45.5942, -72.1061),  // Coyhaique
    ("SCHR", -47.2436, -72.5878),  // Cochrane
    ("SCNT", -51.6708, -72.5286),  // Puerto Natales
    ("SCCI", -53.0025, -70.8544),  // Punta Arenas
    ("SCFM", -53.2536, -70.3192),  // Porvenir
    ("SCGZ", -54.9311, -67.6261),  // Puerto Williams
    ("SCAS", -54.9311, -67.6261),
    ("SCON", -43.1367, -73.6083),
];

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub station: Option<AviationStation>,
    pub metar: Option<Bulletin>,
    pub taf: Option<Bulletin>,
    pub hazards: Vec<Hazard>,
    pub stations: Vec<AviationStation>,
}

pub fn is_chile_icao(id: &str) -> bool {
    let id = id.trim();
    id.len() == 4 && id.as_bytes()[..2].eq_ignore_ascii_case(b"SC")
}

/// Mainland Chile, Rapa Nui, and the Juan Fernández islands.
pub fn in_chile(lat: f64, lon: f64) -> bool {
    in_box(lat, lon, -56.7, -17.0, -76.2, -66.0)
        || in_box(lat, lon, -28.0, -26.5, -110.0, -108.5)
        || in_box(lat, lon, -34.0, -33.3, -79.3, -78.5)
}

fn in_box(lat: f64, lon: f64, lat0: f64, lat1: f64, lon0: f64, lon1: f64) -> bool {
    (lat0..=lat1).contains(&lat) && (lon0..=lon1).contains(&lon)
}

pub fn coords(icao: &str) -> Option<(f64, f64)> {
    let id = icao.trim().to_ascii_uppercase();
    AERODROMES
        .iter()
        .find(|(code, _, _)| *code == id)
        .map(|(_, lat, lon)| (*lat, *lon))
}

pub fn nearby(lat: f64, lon: f64) -> Vec<AviationStation> {
    let mut stations: Vec<AviationStation> = AERODROMES
        .iter()
        .map(|(id, alat, alon)| AviationStation {
            id: (*id).to_owned(),
            lat: *alat,
            lon: *alon,
            distance_km: great_circle_km(lat, lon, *alat, *alon),
        })
        .filter(|s| s.distance_km <= NEAR_KM)
        .collect();
    stations.sort_by(|a, b| a.distance_km.total_cmp(&b.distance_km));
    stations.truncate(MAX_STATIONS);
    stations
}

pub async fn fetch_station(http: &reqwest::Client, base: &str, icao: &str) -> io::Result<Report> {
    let icao = icao.trim().to_ascii_uppercase();
    let (lat, lon) = coords(&icao).unwrap_or((0.0, 0.0));
    let metar_html = get_html(http, base, &format!("/metar?designador={icao}")).await?;
    let taf_html = get_html(http, base, &format!("/taf?designador={icao}"))
        .await
        .unwrap_or_default();
    let notam_html = get_html(http, base, &format!("/notam?designador={icao}"))
        .await
        .unwrap_or_default();
    let sigmet_html = get_html(http, base, "/sigmet").await.unwrap_or_default();
    assemble(
        &icao,
        lat,
        lon,
        Pages {
            metar: &metar_html,
            taf: &taf_html,
            notam: &notam_html,
            sigmet: &sigmet_html,
        },
        lat,
        lon,
    )
}

pub async fn fetch_view(
    http: &reqwest::Client,
    base: &str,
    lat: f64,
    lon: f64,
) -> io::Result<Report> {
    let fir_html = get_html(http, base, "/metar?metodo=fir")
        .await
        .unwrap_or_default();
    let sigmet_html = get_html(http, base, "/sigmet").await.unwrap_or_default();
    let fir = parse_fir_metars(&fir_html);
    let mut stations = nearby(lat, lon);
    if stations.is_empty()
        && let Some((alat, alon)) = coords("SCEL")
    {
        stations = nearby(alat, alon);
    }
    let nearest = stations
        .iter()
        .find(|s| fir.iter().any(|(id, _)| id == &s.id))
        .or_else(|| stations.first())
        .cloned();
    let Some(station) = nearest else {
        return Ok(Report {
            stations,
            hazards: parse_sigmets(&sigmet_html),
            ..Report::default()
        });
    };
    let icao = station.id.clone();
    let taf_html = get_html(http, base, &format!("/taf?designador={icao}"))
        .await
        .unwrap_or_default();
    let notam_html = get_html(http, base, &format!("/notam?designador={icao}"))
        .await
        .unwrap_or_default();
    let metar_html = fir
        .iter()
        .find(|(id, _)| id == &icao)
        .map(|(_, raw)| raw.clone())
        .unwrap_or_default();
    let mut report = assemble(
        &icao,
        station.lat,
        station.lon,
        Pages {
            metar: &metar_html,
            taf: &taf_html,
            notam: &notam_html,
            sigmet: &sigmet_html,
        },
        lat,
        lon,
    )?;
    if report.metar.is_none() && !metar_html.is_empty() {
        report.metar = Some(bulletin(&metar_html, time_from_metar(&metar_html)));
    }
    if report.stations.is_empty() {
        report.stations = stations;
    }
    Ok(report)
}

struct Pages<'a> {
    metar: &'a str,
    taf: &'a str,
    notam: &'a str,
    sigmet: &'a str,
}

fn assemble(
    icao: &str,
    mut lat: f64,
    mut lon: f64,
    pages: Pages<'_>,
    view_lat: f64,
    view_lon: f64,
) -> io::Result<Report> {
    let metar = parse_metar(pages.metar).or_else(|| {
        let plain = collapse_ws(&decode_entities(&strip_tags(pages.metar)));
        extract_bulletin(&plain, &["METAR ", "SPECI "]).map(|raw| {
            bulletin(
                &raw,
                time_from_header(pages.metar).or_else(|| time_from_metar(&raw)),
            )
        })
    });
    let taf = parse_taf(pages.taf);
    let mut hazards = parse_notams(pages.notam);
    hazards.extend(parse_sigmets(pages.sigmet));
    if lat == 0.0
        && lon == 0.0
        && let Some((found_lat, found_lon)) = hazards
            .iter()
            .find_map(|h| h.coords.first().map(|p| (p.lat, p.lon)))
    {
        lat = found_lat;
        lon = found_lon;
    }
    let mut stations = if in_chile(view_lat, view_lon) || in_chile(lat, lon) {
        nearby(
            if lat == 0.0 && lon == 0.0 {
                view_lat
            } else {
                lat
            },
            if lat == 0.0 && lon == 0.0 {
                view_lon
            } else {
                lon
            },
        )
    } else {
        nearby(view_lat, view_lon)
    };
    if !stations.iter().any(|s| s.id == icao) && (lat != 0.0 || lon != 0.0) {
        stations.insert(
            0,
            AviationStation {
                id: icao.to_owned(),
                lat,
                lon,
                distance_km: great_circle_km(view_lat, view_lon, lat, lon),
            },
        );
        stations.truncate(MAX_STATIONS);
    }
    let station = if lat == 0.0 && lon == 0.0 && metar.is_none() && taf.is_none() {
        None
    } else {
        Some(AviationStation {
            id: icao.to_owned(),
            lat,
            lon,
            distance_km: great_circle_km(view_lat, view_lon, lat, lon),
        })
    };
    Ok(Report {
        station,
        metar,
        taf,
        hazards,
        stations,
    })
}

async fn get_html(http: &reqwest::Client, base: &str, path: &str) -> io::Result<String> {
    let url = format!("{}{path}", base.trim_end_matches('/'));
    let response = http.get(&url).send().await.map_err(io::Error::other)?;
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
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

pub fn parse_metar(html: &str) -> Option<Bulletin> {
    let plain = collapse_ws(&decode_entities(&strip_tags(html)));
    let raw = extract_bulletin(&plain, &["METAR ", "SPECI "])?;
    Some(bulletin(
        &raw,
        time_from_header(html).or_else(|| time_from_metar(&raw)),
    ))
}

pub fn parse_taf(html: &str) -> Option<Bulletin> {
    let plain = collapse_ws(&decode_entities(&strip_tags(html)));
    let raw = extract_bulletin(&plain, &["TAF "])?;
    let time = time_from_header(html).or_else(|| time_from_metar(&raw));
    let (valid_from, valid_to) = taf_window(&raw);
    Some(Bulletin {
        raw,
        time: time.unwrap_or_default(),
        valid_from,
        valid_to,
        category: String::new(),
    })
}

pub fn parse_fir_metars(html: &str) -> Vec<(String, String)> {
    let plain = decode_entities(&strip_tags(html));
    let mut out = Vec::new();
    let mut rest = plain.as_str();
    while let Some(at) = find_kind(rest, &["METAR ", "SPECI "]) {
        let slice = &rest[at..];
        if let Some(raw) = take_bulletin(slice) {
            if let Some(id) = icao_from_bulletin(&raw)
                && !out.iter().any(|(have, _)| have == &id)
            {
                out.push((id, raw));
            }
            rest = &slice[1..];
        } else {
            break;
        }
    }
    out
}

pub fn parse_notams(html: &str) -> Vec<Hazard> {
    let lower = html.to_ascii_lowercase();
    let mut hazards = Vec::new();
    let mut search = 0;
    while let Some(rel) = lower[search..].find("notam_raw") {
        let start = search + rel;
        let after = start + 9;
        let next = lower[after..]
            .find("notam_raw")
            .map(|n| after + n)
            .unwrap_or(html.len());
        if let Some(hazard) = notam_from_html(&html[start..next]) {
            hazards.push(hazard);
            if hazards.len() >= MAX_NOTAMS {
                break;
            }
        }
        search = next;
    }
    if hazards.is_empty()
        && let Some(hazard) = notam_from_html(html)
    {
        hazards.push(hazard);
    }
    hazards
}

pub fn parse_sigmets(html: &str) -> Vec<Hazard> {
    let plain = collapse_ws(&decode_entities(&strip_tags(html)));
    let mut hazards = Vec::new();
    let mut rest = plain.as_str();
    while let Some(at) = rest.find("SIGMET") {
        let slice = &rest[at..];
        let end = slice
            .find(" SIGMET")
            .or_else(|| slice.find(" AIRMET"))
            .unwrap_or(slice.len().min(800));
        let raw = slice[..end].trim().to_owned();
        if raw.len() > 8 && !raw.contains("SIGMET/Mapa") {
            hazards.push(Hazard {
                kind: "sigmet".into(),
                hazard: String::new(),
                raw,
                coords: Vec::new(),
                valid_from: String::new(),
                valid_to: String::new(),
            });
        }
        rest = if end < slice.len() { &slice[end..] } else { "" };
        if rest.is_empty() {
            break;
        }
    }
    hazards
}

fn notam_from_html(html: &str) -> Option<Hazard> {
    let plain = collapse_ws(&decode_entities(&strip_tags(html)));
    let start = plain.find("NOTAM")?;
    let before = plain[..start].trim_end();
    let series_at = before
        .rfind(|c: char| c.is_whitespace())
        .map(|i| i + 1)
        .unwrap_or(0);
    let raw = plain[series_at..].trim();
    if raw.len() < 10 {
        return None;
    }
    let q_line = raw.split("Q)").nth(1).unwrap_or("");
    let location = q_line
        .split(|c: char| c == '/' || c.is_whitespace())
        .find_map(parse_q_location);
    let hazard = q_line
        .split('/')
        .nth(1)
        .unwrap_or("")
        .chars()
        .take(5)
        .collect::<String>();
    let (valid_from, valid_to) = notam_window(raw);
    Some(Hazard {
        kind: "notam".into(),
        hazard,
        raw: raw.chars().take(800).collect(),
        coords: location
            .map(|(lat, lon, radius_nm)| circle(lat, lon, radius_nm.max(1.0)))
            .unwrap_or_default(),
        valid_from,
        valid_to,
    })
}

pub fn parse_q_location(field: &str) -> Option<(f64, f64, f64)> {
    let field = field.trim();
    if field.len() != 14 {
        return None;
    }
    let bytes = field.as_bytes();
    if !bytes[0..4].iter().all(|b| b.is_ascii_digit())
        || !matches!(bytes[4], b'N' | b'S' | b'n' | b's')
        || !bytes[5..10].iter().all(|b| b.is_ascii_digit())
        || !matches!(bytes[10], b'E' | b'W' | b'e' | b'w')
        || !bytes[11..14].iter().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let lat_d: f64 = field[0..2].parse().ok()?;
    let lat_m: f64 = field[2..4].parse().ok()?;
    let lon_d: f64 = field[5..8].parse().ok()?;
    let lon_m: f64 = field[8..10].parse().ok()?;
    let radius: f64 = field[11..14].parse().ok()?;
    let mut lat = lat_d + lat_m / 60.0;
    let mut lon = lon_d + lon_m / 60.0;
    if bytes[4] == b'S' || bytes[4] == b's' {
        lat = -lat;
    }
    if bytes[10] == b'W' || bytes[10] == b'w' {
        lon = -lon;
    }
    Some((lat, lon, radius))
}

fn circle(lat: f64, lon: f64, radius_nm: f64) -> Vec<Point> {
    let d = (radius_nm * 1.852) / 6371.0;
    let phi = lat.to_radians();
    let lambda = lon.to_radians();
    (0..16)
        .map(|i| {
            let bearing = (i as f64) * std::f64::consts::TAU / 16.0;
            let lat2 = (phi.sin() * d.cos() + phi.cos() * d.sin() * bearing.cos()).asin();
            let lon2 = lambda
                + (bearing.sin() * d.sin() * phi.cos()).atan2(d.cos() - phi.sin() * lat2.sin());
            Point {
                lat: lat2.to_degrees(),
                lon: lon2.to_degrees(),
            }
        })
        .collect()
}

fn extract_bulletin(plain: &str, kinds: &[&str]) -> Option<String> {
    let at = find_kind(plain, kinds)?;
    take_bulletin(&plain[at..])
}

fn find_kind(plain: &str, kinds: &[&str]) -> Option<usize> {
    kinds
        .iter()
        .filter_map(|k| {
            let mut from = 0;
            while let Some(rel) = plain[from..].find(k) {
                let at = from + rel;
                let prev_ok = at == 0
                    || plain.as_bytes()[at - 1].is_ascii_whitespace()
                    || plain.as_bytes()[at - 1] == b':';
                let rest = plain[at + k.len()..].trim_start();
                let icao = rest.len() >= 4
                    && rest[..4].bytes().all(|b| b.is_ascii_alphabetic())
                    && rest[4..]
                        .chars()
                        .next()
                        .is_none_or(|c| c.is_whitespace() || c.is_ascii_digit());
                if prev_ok && icao {
                    return Some(at);
                }
                from = at + 1;
            }
            None
        })
        .min()
}

fn take_bulletin(slice: &str) -> Option<String> {
    let mut end = slice.len();
    for stop in [
        " Viento:",
        " Visibilidad:",
        " Período",
        " TAF del",
        " METAR del",
        " Para mayor",
    ] {
        if let Some(i) = slice.find(stop) {
            end = end.min(i);
        }
    }
    // A following bulletin of the same kind.
    if let Some(i) = slice[6..]
        .find(" METAR ")
        .or_else(|| slice[6..].find(" SPECI "))
        .or_else(|| slice[6..].find(" TAF "))
    {
        end = end.min(6 + i);
    }
    let raw = slice[..end].trim().trim_end_matches(':').trim();
    if raw.len() < 8 {
        None
    } else {
        Some(collapse_ws(raw))
    }
}

fn icao_from_bulletin(raw: &str) -> Option<String> {
    raw.split_whitespace()
        .nth(1)
        .filter(|id| id.len() == 4 && id.bytes().all(|b| b.is_ascii_alphabetic()))
        .map(|id| id.to_ascii_uppercase())
}

fn bulletin(raw: &str, time: Option<String>) -> Bulletin {
    Bulletin {
        raw: raw.to_owned(),
        time: time.unwrap_or_default(),
        valid_from: String::new(),
        valid_to: String::new(),
        category: String::new(),
    }
}

fn time_from_header(html: &str) -> Option<String> {
    let plain = collapse_ws(&decode_entities(&strip_tags(html)));
    // "METAR del 11-09-2026 a las 15:00 UTC" or "TAF del 11/09/2026 a las 10:00 UTC"
    let del = plain.find(" del ")?;
    let rest = &plain[del + 5..];
    let digits: String = rest
        .chars()
        .take(40)
        .filter(|c| c.is_ascii_digit())
        .collect();
    if digits.len() < 12 {
        return None;
    }
    let day = &digits[0..2];
    let month = &digits[2..4];
    let year = &digits[4..8];
    let hour = &digits[8..10];
    let minute = &digits[10..12];
    Some(format!("{year}-{month}-{day}T{hour}:{minute}:00Z"))
}

fn time_from_metar(raw: &str) -> Option<String> {
    let token = raw.split_whitespace().nth(2)?;
    if token.len() != 7 || !token.ends_with('Z') {
        return None;
    }
    let dd: u32 = token[0..2].parse().ok()?;
    let hh: u32 = token[2..4].parse().ok()?;
    let mm: u32 = token[4..6].parse().ok()?;
    let now = chrono::Utc::now();
    let mut date = now.date_naive();
    if dd != date.day() {
        if let Some(prev) = date.pred_opt()
            && prev.day() == dd
        {
            date = prev;
        }
        if let Some(next) = date.succ_opt()
            && next.day() == dd
        {
            date = next;
        }
    }
    chrono::NaiveDate::from_ymd_opt(date.year(), date.month(), dd)
        .and_then(|d| d.and_hms_opt(hh, mm, 0))
        .map(|t| {
            t.and_utc()
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
        })
}

fn taf_window(raw: &str) -> (String, String) {
    // TAF SCEL 111000Z 1112/1212 ...
    let Some(token) = raw
        .split_whitespace()
        .find(|t| t.len() == 9 && t.contains('/'))
    else {
        return (String::new(), String::new());
    };
    let (from, to) = token.split_once('/').unwrap();
    (ddhh_iso(from), ddhh_iso(to))
}

fn ddhh_iso(token: &str) -> String {
    if token.len() != 4 {
        return String::new();
    }
    let Ok(dd) = token[0..2].parse::<u32>() else {
        return String::new();
    };
    let Ok(hh) = token[2..4].parse::<u32>() else {
        return String::new();
    };
    let now = chrono::Utc::now().date_naive();
    chrono::NaiveDate::from_ymd_opt(now.year(), now.month(), dd)
        .and_then(|d| d.and_hms_opt(hh, 0, 0))
        .map(|t| {
            t.and_utc()
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
        })
        .unwrap_or_default()
}

fn notam_window(raw: &str) -> (String, String) {
    let from = field_after(raw, "B)").and_then(|t| yymmddhhmm(&t));
    let to = field_after(raw, "C)").and_then(|t| {
        if t.starts_with("PERM") {
            None
        } else {
            yymmddhhmm(&t)
        }
    });
    (from.unwrap_or_default(), to.unwrap_or_default())
}

fn field_after(raw: &str, tag: &str) -> Option<String> {
    let rest = raw.split(tag).nth(1)?.trim();
    let token = rest.split_whitespace().next()?.to_owned();
    Some(token)
}

fn yymmddhhmm(token: &str) -> Option<String> {
    if token.len() != 10 || !token.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let year: i32 = 2000 + token[0..2].parse::<i32>().ok()?;
    let month: u32 = token[2..4].parse().ok()?;
    let day: u32 = token[4..6].parse().ok()?;
    let hour: u32 = token[6..8].parse().ok()?;
    let minute: u32 = token[8..10].parse().ok()?;
    chrono::NaiveDate::from_ymd_opt(year, month, day)
        .and_then(|d| d.and_hms_opt(hour, minute, 0))
        .map(|t| {
            t.and_utc()
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
        })
}

fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

fn decode_entities(text: &str) -> String {
    text.replace("&nbsp;", " ")
        .replace("&oacute;", "ó")
        .replace("&Oacute;", "Ó")
        .replace("&aacute;", "á")
        .replace("&eacute;", "é")
        .replace("&iacute;", "í")
        .replace("&uacute;", "ú")
        .replace("&ntilde;", "ñ")
        .replace("&deg;", "°")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

fn collapse_ws(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut gap = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            gap = true;
            continue;
        }
        if gap && !out.is_empty() {
            out.push(' ');
        }
        gap = false;
        out.push(ch);
    }
    out
}

pub fn great_circle_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let (dp, dl) = ((lat2 - lat1).to_radians(), (lon2 - lon1).to_radians());
    let h = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * 6371.0 * h.clamp(0.0, 1.0).sqrt().asin()
}

#[cfg(test)]
mod tests {
    use super::*;

    const METAR_HTML: &str = r#"
        <div class="taf_h1 metar1">METAR del 11-09-2026 a las 15:00 UTC</div>
        <div class="taf_h3 codificacion metar1">
            METAR SCEL 111500Z 19005KT 9999 SCT020 BKN100 13/07 Q1021 NOSIG
        </div>
        <div class="taf_p">Viento: direcci&oacute;n 190 grados</div>
    "#;

    const TAF_HTML: &str = r#"
        <div class="taf_h3 codificacion">
        <abbr>TAF</abbr>: TAF SCEL 111000Z 1112/1212 VRB02KT 9999 FEW020 BKN040
        </div>
        <div>TAF del 11/09/2026 a las 10:00 UTC</div>
        <div class="taf_p">Período de validez</div>
    "#;

    const NOTAM_HTML: &str = r#"
        <tr class="notam_raw">
            <td class="codificacion">
                A2586/26 NOTAMN<br />
                Q)SCEZ/QATLT/IV/NBO/AE/000/999/3324S07048W090<br />
                A)SCEL B)2609171200 C)2609172100<br />
                E) POSS DLA IN SANTIAGO TMA
            </td>
        </tr>
        <tr class="notam_raw">
            <td>
                A2579/26 NOTAMN<br />
                Q)SCEZ/QMRLC/IV/NBO/A/000/999/3324S07048W005<br />
                A)SCEL B)2610121200 C)2610121400<br />
                E) RWY 17R/35L CLSD
            </td>
        </tr>
    "#;

    const FIR_HTML: &str = r#"
        <div class="taf_h3 codificacion metar1">METAR SCHR 111500Z 00000KT 9999 FEW060 07/M04 Q1031</div>
        <div class="taf_h3">METAR SCIP 111500Z 13007KT 9999 FEW011 OVC045 19/18 Q1016 NOSIG</div>
        <div class="taf_h3">METAR SCIE 111500Z VRB04KT 9999 BKN025 12/07 Q1024</div>
    "#;

    #[test]
    fn scel_metar_comes_off_the_ifis_page() {
        let metar = parse_metar(METAR_HTML).unwrap();
        assert_eq!(
            metar.raw,
            "METAR SCEL 111500Z 19005KT 9999 SCT020 BKN100 13/07 Q1021 NOSIG"
        );
        assert!(metar.raw.contains("Q1021 NOSIG"));
        assert_eq!(metar.time, "2026-09-11T15:00:00Z");
    }

    #[test]
    fn scel_taf_keeps_the_issued_bulletin() {
        let taf = parse_taf(TAF_HTML).unwrap();
        assert!(taf.raw.starts_with("TAF SCEL 111000Z"));
        assert!(taf.raw.contains("1112/1212"));
        assert_eq!(taf.time, "2026-09-11T10:00:00Z");
    }

    #[test]
    fn notam_q_line_becomes_a_circle_on_santiago() {
        let hazards = parse_notams(NOTAM_HTML);
        assert_eq!(hazards.len(), 2);
        assert_eq!(hazards[0].kind, "notam");
        assert_eq!(hazards[0].hazard, "QATLT");
        assert!(hazards[0].raw.contains("A2586/26"));
        assert_eq!(hazards[0].coords.len(), 16);
        let lat = hazards[0].coords.iter().map(|p| p.lat).sum::<f64>() / 16.0;
        let lon = hazards[0].coords.iter().map(|p| p.lon).sum::<f64>() / 16.0;
        assert!((lat - (-33.4)).abs() < 0.2);
        assert!((lon - (-70.8)).abs() < 0.2);
        assert_eq!(hazards[0].valid_from, "2026-09-17T12:00:00Z");
        assert_eq!(hazards[1].hazard, "QMRLC");
        assert_eq!(hazards[1].valid_to, "2026-10-12T14:00:00Z");
    }

    #[test]
    fn fir_listing_keeps_the_first_metar_per_station() {
        let fir = parse_fir_metars(FIR_HTML);
        assert_eq!(fir[0].0, "SCHR");
        assert!(fir.iter().any(|(id, _)| id == "SCIP"));
        assert!(
            fir.iter()
                .any(|(id, raw)| id == "SCIE" && raw.contains("BKN025"))
        );
    }

    #[test]
    fn chile_is_sc_prefix_not_seychelles() {
        assert!(is_chile_icao("SCEL"));
        assert!(is_chile_icao("scip"));
        assert!(!is_chile_icao("KJAX"));
        assert!(!is_chile_icao("FSIA"));
        assert!(in_chile(-33.45, -70.67));
        assert!(in_chile(-27.16, -109.43));
        assert!(!in_chile(36.24, -79.98));
        assert_eq!(coords("SCEL").unwrap().0, -33.3931);
        assert!(nearby(-33.45, -70.67).iter().any(|s| s.id == "SCEL"));
    }

    #[test]
    fn q_location_parses_degrees_minutes_and_radius() {
        let (lat, lon, r) = parse_q_location("3324S07048W090").unwrap();
        assert!((lat - (-33.4)).abs() < 0.01);
        assert!((lon - (-70.8)).abs() < 0.01);
        assert_eq!(r, 90.0);
        assert!(parse_q_location("bad").is_none());
    }
}

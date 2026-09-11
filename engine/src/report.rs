//! Printable Lambert conformal conic charts of the selected box
//! (`docs/protocol.md`, `export_report`). Radar is sampled from the current
//! sweep texture with the same gate geometry the shader uses; geography is
//! the embedded Natural Earth polylines. Writes under
//! `$XDG_DATA_HOME/omastorm/reports/`.

use crate::lcc::Lambert;
use crate::protocol::{Frame, is_report_path};
use crate::sweep;
use crate::tiles::{self, QUANTUM};
use std::{
    env,
    fs::{self, OpenOptions},
    io::{self, Write},
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
use tiny_skia::{Color, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, Transform};

const R_M: f32 = 6_371_000.0;
const EARTH_M: f32 = R_M * 4.0 / 3.0;
const DEFAULT_WIDTH: u32 = 1280;
const MIN_WIDTH: u32 = 480;
const MAX_WIDTH: u32 = 2048;
const HEADER: u32 = 40;
const FOOTER: u32 = 52;
const PAPER: [u8; 3] = [244, 241, 234];
const INK: [u8; 3] = [28, 26, 23];
const BOUNDARY: [u8; 4] = [42, 40, 36, 220];
const COAST: [u8; 4] = [58, 90, 106, 200];
const RING: [u8; 4] = [80, 70, 60, 160];
const RING_KM: [f64; 3] = [50.0, 100.0, 150.0];

#[derive(Clone, Debug)]
pub struct Request {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
    pub layers: Vec<String>,
    pub width: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct Accepted {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
    pub layers: Vec<String>,
    pub width: u32,
}

#[derive(Clone, Debug)]
pub struct Job {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
    pub layers: Vec<String>,
    pub width: u32,
    pub frame: Frame,
    pub texture: Vec<u8>,
    pub lut: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct Ready {
    pub path: String,
    pub layers: Vec<String>,
    pub width: u32,
    pub height: u32,
}

impl Job {
    pub fn validate(
        west: f64,
        south: f64,
        east: f64,
        north: f64,
        layers: Vec<String>,
        width: Option<u32>,
        frame: &Frame,
    ) -> Result<Accepted, String> {
        if !(-90.0..=90.0).contains(&south) || !(-90.0..=90.0).contains(&north) || south >= north {
            return Err("export_report needs south < north, both latitudes in [-90, 90].".into());
        }
        if !(-180.0..=180.0).contains(&west) || !(-180.0..=180.0).contains(&east) || west >= east {
            return Err("export_report needs west < east, both longitudes in [-180, 180].".into());
        }
        if north - south > 40.0 || east - west > 60.0 {
            return Err(
                "export_report box is larger than 40° of latitude or 60° of longitude.".into(),
            );
        }
        if layers.is_empty() {
            return Err("export_report needs at least one layer.".into());
        }
        let mut wanted = Vec::new();
        for layer in layers {
            let name = match layer.as_str() {
                "ref" | "storms" => "ref",
                "basemap" => "basemap",
                "rings" => "rings",
                other => {
                    return Err(format!(
                        "Unknown report layer \"{other}\". This build can export ref (storms), basemap, and rings."
                    ));
                }
            };
            if !wanted.iter().any(|have| have == name) {
                wanted.push(name.to_string());
            }
        }
        if frame.scan_time.is_empty() || frame.rays <= 1 || frame.gates <= 1 {
            return Err("No sweep to export yet.".into());
        }
        let width = width.unwrap_or(DEFAULT_WIDTH);
        if !(MIN_WIDTH..=MAX_WIDTH).contains(&width) {
            return Err(format!(
                "export_report width must be between {MIN_WIDTH} and {MAX_WIDTH}."
            ));
        }
        Ok(Accepted {
            west,
            south,
            east,
            north,
            layers: wanted,
            width,
        })
    }

    pub fn run(self) -> io::Result<Ready> {
        let (texture_w, texture_h, texture) = decode_rgba(&self.texture)?;
        let (_, _, lut) = decode_rgba(&self.lut)?;
        if texture_w != self.frame.gates || texture_h != self.frame.rays {
            return Err(io::Error::other(
                "sweep texture size does not match the frame",
            ));
        }
        if lut.len() < 3600 * 4 {
            return Err(io::Error::other("azimuth lookup is short"));
        }
        let palette = parse_palette(&self.frame.palette);
        let map = Map::new(self.west, self.south, self.east, self.north, self.width);
        let map_w = map.w;
        let map_h = map.h;
        let height = map_h + HEADER + FOOTER;
        let mut pixmap =
            Pixmap::new(map_w, height).ok_or_else(|| io::Error::other("chart size"))?;
        pixmap.fill(Color::from_rgba8(PAPER[0], PAPER[1], PAPER[2], 255));
        let paint_ref = self.layers.iter().any(|layer| layer == "ref");
        let pixels = pixmap.pixels_mut();
        for y in 0..map_h {
            for x in 0..map_w {
                let (lon, lat) = map.lon_lat(x, y);
                if lon < self.west || lon > self.east || lat < self.south || lat > self.north {
                    continue;
                }
                if !paint_ref {
                    continue;
                }
                if let Some(rgb) = sample(
                    lon,
                    lat,
                    &self.frame,
                    &texture,
                    texture_w as usize,
                    &lut,
                    &palette,
                ) {
                    let i = ((HEADER + y) * map_w + x) as usize;
                    pixels[i] =
                        tiny_skia::PremultipliedColorU8::from_rgba(rgb[0], rgb[1], rgb[2], 255)
                            .unwrap_or(pixels[i]);
                }
            }
        }
        if self.layers.iter().any(|layer| layer == "basemap") {
            stroke_geography(&mut pixmap, &map);
            label_places(&mut pixmap, &map);
        }
        if self.layers.iter().any(|layer| layer == "rings") {
            stroke_rings(&mut pixmap, &map, self.frame.site.lon, self.frame.site.lat);
        }
        draw_chrome(&mut pixmap, &self, map_w, map_h, &palette);
        let png = sweep::png(map_w, height, pixmap.data())?;
        let path = publish(&png, &self.frame)?;
        Ok(Ready {
            path,
            layers: self.layers,
            width: map_w,
            height,
        })
    }
}

struct Map {
    lcc: Lambert,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    w: u32,
    h: u32,
    west: f64,
    south: f64,
    east: f64,
    north: f64,
}

impl Map {
    fn new(west: f64, south: f64, east: f64, north: f64, width: u32) -> Self {
        let lcc = Lambert::for_box(west, south, east, north);
        let (x0, y0, x1, y1) = extent(&lcc, west, south, east, north);
        let h = ((f64::from(width) * (y1 - y0) / (x1 - x0)).round() as u32).clamp(240, 1600);
        Self {
            lcc,
            x0,
            y0,
            x1,
            y1,
            w: width,
            h,
            west,
            south,
            east,
            north,
        }
    }

    fn lon_lat(&self, x: u32, y: u32) -> (f64, f64) {
        let fx = self.x0 + (f64::from(x) + 0.5) / f64::from(self.w) * (self.x1 - self.x0);
        let fy = self.y1 - (f64::from(y) + 0.5) / f64::from(self.h) * (self.y1 - self.y0);
        self.lcc.inverse(fx, fy)
    }

    fn pixel(&self, lon: f64, lat: f64) -> (f32, f32) {
        let (x, y) = self.lcc.forward(lon, lat);
        let px = ((x - self.x0) / (self.x1 - self.x0) * f64::from(self.w)) as f32;
        let py = HEADER as f32 + ((self.y1 - y) / (self.y1 - self.y0) * f64::from(self.h)) as f32;
        (px, py)
    }
}

fn extent(lcc: &Lambert, west: f64, south: f64, east: f64, north: f64) -> (f64, f64, f64, f64) {
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for i in 0..=8 {
        let fx = f64::from(i) / 8.0;
        for (lon, lat) in [
            (west + fx * (east - west), south),
            (west + fx * (east - west), north),
            (west, south + fx * (north - south)),
            (east, south + fx * (north - south)),
        ] {
            let (x, y) = lcc.forward(lon, lat);
            xs.push(x);
            ys.push(y);
        }
    }
    let (xmin, xmax) = min_max(&xs);
    let (ymin, ymax) = min_max(&ys);
    let pad_x = (xmax - xmin).max(1.0) * 0.02;
    let pad_y = (ymax - ymin).max(1.0) * 0.02;
    (xmin - pad_x, ymin - pad_y, xmax + pad_x, ymax + pad_y)
}

fn min_max(values: &[f64]) -> (f64, f64) {
    let mut min = values[0];
    let mut max = values[0];
    for &value in values {
        min = min.min(value);
        max = max.max(value);
    }
    (min, max)
}

fn sample(
    lon: f64,
    lat: f64,
    frame: &Frame,
    texture: &[u8],
    gates: usize,
    lut: &[u8],
    palette: &[[u8; 3]],
) -> Option<[u8; 3]> {
    let (gate, azimuth) = site_gate(frame, lon, lat)?;
    if gate < -0.5 || gate > frame.gates as f32 - 0.5 {
        return None;
    }
    let entry = ((azimuth * 10.0).floor() as i32).rem_euclid(3600) as usize;
    let row = u16::from(lut[entry * 4]) | (u16::from(lut[entry * 4 + 1]) << 8);
    let col = gate.round() as usize;
    if usize::from(row) >= frame.rays as usize || col >= gates {
        return None;
    }
    let i = (usize::from(row) * gates + col) * 4;
    let (class, status, code) = (texture[i], texture[i + 1], texture[i + 2]);
    if class == 0 {
        return if status & sweep::FOLDED != 0 {
            Some([200, 200, 200])
        } else {
            None
        };
    }
    let _ = code;
    palette.get((class as usize).saturating_sub(1)).copied()
}

fn site_gate(frame: &Frame, lon: f64, lat: f64) -> Option<(f32, f32)> {
    let lat0 = frame.site.lat.to_radians();
    let lat1 = lat.to_radians();
    let d_lat = (lat - frame.site.lat).to_radians();
    let d_lon = (lon - frame.site.lon).to_radians();
    let h = (d_lat * 0.5).sin().powi(2) + lat0.cos() * lat1.cos() * (d_lon * 0.5).sin().powi(2);
    let ground_m = 2.0 * f64::from(R_M) * h.clamp(0.0, 1.0).sqrt().asin();
    let x = d_lon.sin() * lat1.cos();
    let y = lat0.cos() * lat1.sin() - lat0.sin() * lat1.cos() * d_lon.cos();
    let mut azimuth = x.atan2(y).to_degrees();
    if azimuth < 0.0 {
        azimuth += 360.0;
    }
    let arc = ground_m as f32 / EARTH_M;
    let elev = (frame.elevation_deg as f32).to_radians();
    if elev + arc >= std::f32::consts::FRAC_PI_2 {
        return None;
    }
    let slant_m = EARTH_M * arc.sin() / (elev + arc).cos();
    let gate = (slant_m - frame.first_gate_m as f32) / frame.gate_spacing_m as f32;
    Some((gate, azimuth as f32))
}

fn stroke_geography(pixmap: &mut Pixmap, map: &Map) {
    let geography = tiles::Geography::embedded();
    let fine = map.north - map.south < 12.0;
    let q = |degrees: f64| (degrees / QUANTUM).round() as i32;
    let box_bounds = [q(map.west), q(map.south), q(map.east), q(map.north)];
    for (layer, color, width) in [(0, BOUNDARY, 1.35_f32), (1, COAST, 1.15)] {
        let mut builder = PathBuilder::new();
        for (points, bounds) in geography.lines(fine, layer) {
            if bounds[2] < box_bounds[0]
                || bounds[0] > box_bounds[2]
                || bounds[3] < box_bounds[1]
                || bounds[1] > box_bounds[3]
            {
                continue;
            }
            let mut started = false;
            for &(qx, qy) in points {
                let (px, py) = map.pixel(f64::from(qx) * QUANTUM, f64::from(qy) * QUANTUM);
                if !started {
                    builder.move_to(px, py);
                    started = true;
                } else {
                    builder.line_to(px, py);
                }
            }
        }
        if let Some(path) = builder.finish() {
            let mut paint = Paint::default();
            paint.set_color_rgba8(color[0], color[1], color[2], color[3]);
            paint.anti_alias = true;
            let stroke = Stroke {
                width,
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                ..Stroke::default()
            };
            pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
    }
}

fn stroke_rings(pixmap: &mut Pixmap, map: &Map, lon0: f64, lat0: f64) {
    let mut paint = Paint::default();
    paint.set_color_rgba8(RING[0], RING[1], RING[2], RING[3]);
    paint.anti_alias = true;
    let stroke = Stroke {
        width: 1.0,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };
    for &km in &RING_KM {
        let mut builder = PathBuilder::new();
        for i in 0..=180 {
            let bearing = f64::from(i) * 2.0 * std::f64::consts::PI / 180.0;
            let (lon, lat) = dest(lat0, lon0, km, bearing);
            let (px, py) = map.pixel(lon, lat);
            if i == 0 {
                builder.move_to(px, py);
            } else {
                builder.line_to(px, py);
            }
        }
        if let Some(path) = builder.finish() {
            pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
    }
}

fn dest(lat0: f64, lon0: f64, km: f64, bearing: f64) -> (f64, f64) {
    let angular = km / 6371.0;
    let phi1 = lat0.to_radians();
    let phi2 = (phi1.sin() * angular.cos() + phi1.cos() * angular.sin() * bearing.cos()).asin();
    let lambda2 = lon0.to_radians()
        + (bearing.sin() * angular.sin() * phi1.cos())
            .atan2(angular.cos() - phi1.sin() * phi2.sin());
    (lambda2.to_degrees(), phi2.to_degrees())
}

fn label_places(pixmap: &mut Pixmap, map: &Map) {
    for place in
        tiles::Geography::embedded().places_in_box(map.west, map.south, map.east, map.north, 18)
    {
        let (px, py) = map.pixel(place.lon, place.lat);
        let (px, py) = (px as i32, py as i32);
        dot(pixmap, px, py, INK);
        text(
            pixmap,
            px + 5,
            py - 3,
            &place.name.to_ascii_uppercase(),
            INK,
            1,
        );
    }
}

fn draw_chrome(pixmap: &mut Pixmap, job: &Job, map_w: u32, map_h: u32, palette: &[[u8; 3]]) {
    let site = job
        .frame
        .id
        .split('-')
        .next()
        .unwrap_or("SITE")
        .to_ascii_uppercase();
    let title = format!(
        "OMASTORM  {site}  {}  {:.1} DEG  {}  LCC",
        job.frame.product.to_ascii_uppercase(),
        job.frame.elevation_deg,
        job.frame.scan_time
    );
    text(pixmap, 10, 8, &title, INK, 2);
    let area = format!(
        "{}  TO  {}   {}",
        coord(job.north, job.west),
        coord(job.south, job.east),
        job.layers
            .iter()
            .map(|layer| layer.to_ascii_uppercase())
            .collect::<Vec<_>>()
            .join(" ")
    );
    text(pixmap, 10, 26, &area, INK, 1);
    let y = (HEADER + map_h + 10) as i32;
    if job.layers.iter().any(|layer| layer == "ref") && !palette.is_empty() {
        let band = ((map_w as i32 - 20) / palette.len() as i32).clamp(8, 28);
        for (i, color) in palette.iter().enumerate() {
            fill_rect(pixmap, 10 + i as i32 * band, y, band - 1, 10, *color);
        }
        text(
            pixmap,
            10,
            y + 16,
            &format!(
                "{} {}   NATURAL EARTH",
                job.frame.product_name.to_ascii_uppercase(),
                job.frame.units
            ),
            INK,
            1,
        );
    } else {
        text(pixmap, 10, y + 8, "NATURAL EARTH", INK, 1);
    }
}

fn coord(lat: f64, lon: f64) -> String {
    let ns = if lat >= 0.0 { "N" } else { "S" };
    let ew = if lon >= 0.0 { "E" } else { "W" };
    format!("{:.2}{} {:.2}{}", lat.abs(), ns, lon.abs(), ew)
}

fn parse_palette(palette: &[String]) -> Vec<[u8; 3]> {
    palette
        .iter()
        .filter_map(|hex| {
            let hex = hex.trim_start_matches('#');
            if hex.len() < 6 {
                return None;
            }
            let channel = |i| u8::from_str_radix(&hex[i..i + 2], 16).ok();
            Some([channel(0)?, channel(2)?, channel(4)?])
        })
        .collect()
}

fn decode_rgba(bytes: &[u8]) -> io::Result<(u32, u32, Vec<u8>)> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::ALPHA);
    let mut reader = decoder.read_info().map_err(io::Error::other)?;
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).map_err(io::Error::other)?;
    Ok((info.width, info.height, pixels))
}

fn data_home() -> io::Result<PathBuf> {
    let base = match env::var_os("XDG_DATA_HOME") {
        Some(value) => PathBuf::from(value),
        None => {
            let home = env::var_os("HOME").ok_or_else(|| io::Error::other("HOME is required"))?;
            PathBuf::from(home).join(".local/share")
        }
    };
    if !base.is_absolute() {
        return Err(io::Error::other("XDG_DATA_HOME must be absolute"));
    }
    Ok(base.join("omastorm"))
}

fn publish(bytes: &[u8], frame: &Frame) -> io::Result<String> {
    let dir = data_home()?.join("reports");
    fs::create_dir_all(&dir)?;
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    let revision = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let stem = frame
        .id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>();
    let name = format!("reports/{stem}-lcc-r{revision}.png");
    if !is_report_path(&name) {
        return Err(io::Error::other(format!("refusing report path {name}")));
    }
    let temporary = data_home()?.join(format!("{name}.tmp"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(temporary, data_home()?.join(&name))?;
    let _ = dir;
    Ok(name)
}

fn fill_rect(pixmap: &mut Pixmap, x: i32, y: i32, w: i32, h: i32, rgb: [u8; 3]) {
    let color = tiny_skia::PremultipliedColorU8::from_rgba(rgb[0], rgb[1], rgb[2], 255);
    let Some(color) = color else {
        return;
    };
    let (width, height) = (pixmap.width() as i32, pixmap.height() as i32);
    let pixels = pixmap.pixels_mut();
    for row in y.max(0)..(y + h).min(height) {
        for col in x.max(0)..(x + w).min(width) {
            pixels[(row * width + col) as usize] = color;
        }
    }
}

fn dot(pixmap: &mut Pixmap, x: i32, y: i32, rgb: [u8; 3]) {
    fill_rect(pixmap, x - 1, y - 1, 3, 3, rgb);
}

fn text(pixmap: &mut Pixmap, mut x: i32, y: i32, value: &str, rgb: [u8; 3], scale: i32) {
    let color = tiny_skia::PremultipliedColorU8::from_rgba(rgb[0], rgb[1], rgb[2], 255);
    let Some(color) = color else {
        return;
    };
    let (width, height) = (pixmap.width() as i32, pixmap.height() as i32);
    let pixels = pixmap.pixels_mut();
    for ch in value.chars() {
        let glyph = glyph(ch);
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..5 {
                if bits & (1 << (4 - col)) == 0 {
                    continue;
                }
                let px = x + col * scale;
                let py = y + row as i32 * scale;
                for dy in 0..scale {
                    for dx in 0..scale {
                        let gx = px + dx;
                        let gy = py + dy;
                        if gx >= 0 && gy >= 0 && gx < width && gy < height {
                            pixels[(gy * width + gx) as usize] = color;
                        }
                    }
                }
            }
        }
        x += 6 * scale;
    }
}

/// 5×7 caps for the chart chrome; unknown glyphs are a narrow space.
fn glyph(ch: char) -> [u8; 7] {
    match ch.to_ascii_uppercase() {
        'A' => [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'B' => [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
        'C' => [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
        'D' => [0x1E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1E],
        'E' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
        'F' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10],
        'G' => [0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0E],
        'H' => [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'I' => [0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E],
        'J' => [0x01, 0x01, 0x01, 0x01, 0x11, 0x11, 0x0E],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F],
        'M' => [0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        'O' => [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'P' => [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
        'Q' => [0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D],
        'R' => [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11],
        'S' => [0x0E, 0x11, 0x10, 0x0E, 0x01, 0x11, 0x0E],
        'T' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04],
        'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x1B, 0x11],
        'X' => [0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11],
        'Y' => [0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04],
        'Z' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F],
        '0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        '1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        '2' => [0x0E, 0x11, 0x01, 0x06, 0x08, 0x10, 0x1F],
        '3' => [0x0E, 0x11, 0x01, 0x06, 0x01, 0x11, 0x0E],
        '4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        '5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        '6' => [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        '7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        '9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
        '-' => [0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x0C, 0x0C],
        ':' => [0x00, 0x0C, 0x0C, 0x00, 0x0C, 0x0C, 0x00],
        '/' => [0x01, 0x01, 0x02, 0x04, 0x08, 0x10, 0x10],
        '+' => [0x00, 0x04, 0x04, 0x1F, 0x04, 0x04, 0x00],
        _ => [0; 7],
    }
}

#[cfg(test)]
mod tests {
    use super::Job;
    use crate::protocol::{Frame, FrameStatus, Geometry};

    fn frame() -> Frame {
        Frame {
            id: "KTLX-test".into(),
            product: "REF".into(),
            product_name: "Reflectivity".into(),
            units: "dBZ".into(),
            elevation_deg: 0.5,
            scan_time: "2013-05-20T20:16:43Z".into(),
            sweep_end: "2013-05-20T20:17:00Z".into(),
            status: FrameStatus::Complete,
            texture: "tex/a.png".into(),
            azimuth_lut: "tex/b.png".into(),
            rays: 720,
            gates: 1832,
            first_gate_m: 2125,
            gate_spacing_m: 250,
            scale: 2.0,
            offset: 66.0,
            site: Geometry {
                lat: 35.33,
                lon: -97.28,
                alt_m: 388.0,
            },
            palette: vec!["#34465f".into()],
            bounds: vec![-32, 96],
            kind: String::new(),
            altitude_hpa: 0,
            altitude_name: String::new(),
            layer_source: String::new(),
        }
    }

    #[test]
    fn unknown_layers_are_named() {
        let err = Job::validate(
            -98.0,
            34.0,
            -96.0,
            36.0,
            vec!["pressure".into()],
            None,
            &frame(),
        )
        .unwrap_err();
        assert!(err.contains("pressure"), "{err}");
        assert!(err.contains("ref"), "{err}");
    }

    #[test]
    fn storms_is_reflectivity() {
        let accepted = Job::validate(
            -98.0,
            34.0,
            -96.0,
            36.0,
            vec!["storms".into(), "basemap".into()],
            None,
            &frame(),
        )
        .unwrap();
        assert_eq!(accepted.layers, ["ref", "basemap"]);
        assert_eq!(accepted.width, 1280);
    }
}

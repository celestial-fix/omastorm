//! Local WRF-ARW forecast producer. An explicit `run_wrf` starts
//! `scripts/wrf-forecast.sh` against a Docker image; ordinary `run.sh`
//! never pulls or starts it. Output and working files stay under
//! `$XDG_CACHE_HOME/omastorm/wrf/`.
//!
//! Wall-clock estimates come from domain area, grid spacing, forecast
//! length, vertical levels, and CPU cores. They are order-of-magnitude
//! guidance for a GNU-build ARW container on a desktop, not a reservation.

use crate::protocol::{Wrf, WrfEstimate, WrfStatus};
use std::{env, path::PathBuf, thread::available_parallelism};

/// Reference integration used to pin the cost model: 450×450 km at 15 km
/// (30×30 mass points), 33 levels, 6 h, 4 cores → about 10 minutes of
/// `wrf.exe` on a recent laptop-class CPU.
const REF_CELLS: f64 = 900.0;
const REF_LEVELS: f64 = 33.0;
const REF_STEPS: f64 = 240.0;
const REF_CORES: f64 = 4.0;
const REF_WRF_SEC: f64 = 600.0;

const MIN_SPAN_KM: f64 = 80.0;
const MAX_SPAN_KM: f64 = 2_000.0;
const MIN_DX_KM: f64 = 3.0;
const MAX_DX_KM: f64 = 27.0;
const MIN_HOURS: u32 = 3;
const MAX_HOURS: u32 = 24;
const MIN_LEVELS: u32 = 20;
const MAX_LEVELS: u32 = 45;
const DEFAULT_LEVELS: u32 = 33;
const DEFAULT_HOURS: u32 = 12;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    pub width_km: f64,
    pub height_km: f64,
    pub dx_km: f64,
    pub hours: u32,
    pub cores: u32,
    pub levels: u32,
}

impl Params {
    pub fn from_command(
        width_km: f64,
        height_km: f64,
        dx_km: f64,
        hours: u32,
        cores: u32,
        fallback_span_km: f64,
    ) -> Params {
        let span = if width_km > 0.0 && height_km > 0.0 {
            width_km.max(height_km)
        } else if width_km > 0.0 {
            width_km
        } else {
            fallback_span_km
        };
        Params::from_view(span, hours, cores, dx_km, DEFAULT_LEVELS)
    }

    pub fn from_view(span_km: f64, hours: u32, cores: u32, dx_km: f64, levels: u32) -> Params {
        let span = span_km.clamp(MIN_SPAN_KM, MAX_SPAN_KM);
        let dx = if dx_km > 0.0 {
            dx_km.clamp(MIN_DX_KM, MAX_DX_KM)
        } else {
            suggest_dx_km(span)
        };
        let hrs = if hours == 0 {
            DEFAULT_HOURS
        } else {
            hours.clamp(MIN_HOURS, MAX_HOURS)
        };
        let lev = if levels == 0 {
            DEFAULT_LEVELS
        } else {
            levels.clamp(MIN_LEVELS, MAX_LEVELS)
        };
        Params {
            width_km: span,
            height_km: span,
            dx_km: dx,
            hours: hrs,
            cores: resolve_cores(cores),
            levels: lev,
        }
    }

    pub fn nx(&self) -> u32 {
        grid_side(self.width_km, self.dx_km)
    }

    pub fn ny(&self) -> u32 {
        grid_side(self.height_km, self.dx_km)
    }

    pub fn cells(&self) -> u32 {
        self.nx().saturating_sub(1) * self.ny().saturating_sub(1)
    }

    /// ARW CFL rule of thumb: timestep seconds ≈ 6 × Δx kilometres.
    pub fn dt_sec(&self) -> f64 {
        (6.0 * self.dx_km).clamp(6.0, 90.0)
    }

    pub fn steps(&self) -> u32 {
        ((f64::from(self.hours) * 3600.0) / self.dt_sec())
            .ceil()
            .max(1.0) as u32
    }

    pub fn grib_files(&self) -> u32 {
        self.hours.div_ceil(3) + 1
    }

    pub fn area_km2(&self) -> f64 {
        self.width_km * self.height_km
    }
}

/// Coarser spacing for a wide view so cell count stays runnable.
pub fn suggest_dx_km(span_km: f64) -> f64 {
    if span_km >= 800.0 {
        15.0
    } else if span_km >= 300.0 {
        9.0
    } else {
        3.0
    }
}

pub fn resolve_cores(requested: u32) -> u32 {
    let host = available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(4)
        .clamp(1, 32);
    if requested == 0 {
        host
    } else {
        requested.min(host).max(1)
    }
}

fn grid_side(span_km: f64, dx_km: f64) -> u32 {
    ((span_km / dx_km).round() as u32 + 1).clamp(11, 201)
}

fn parallel_efficiency(cores: u32) -> f64 {
    match cores {
        0..=4 => 1.0,
        5..=8 => 0.85,
        9..=16 => 0.70,
        _ => 0.55,
    }
}

/// Seconds per cell-level-step on one efficient core, from the reference run.
fn wrf_cost_sec() -> f64 {
    REF_WRF_SEC * REF_CORES / (REF_CELLS * REF_LEVELS * REF_STEPS)
}

pub fn estimate(params: Params) -> WrfEstimate {
    let cells = params.cells();
    let steps = params.steps();
    let dt = params.dt_sec();
    let files = params.grib_files();
    let cores = params.cores.max(1);

    // NOMADS GFS 0.25° files are a few hundred MB; 45 s each is a typical
    // desktop fetch. Cached files drop this toward the low end of the band.
    let download_sec = f64::from(files) * 45.0;
    let geogrid_sec = 8.0 + f64::from(cells) / 400.0;
    let ungrib_sec = 20.0 + 8.0 * f64::from(files);
    let metgrid_sec = 15.0 + f64::from(cells) * f64::from(files) / 800.0;
    let real_sec = 20.0 + f64::from(cells) * f64::from(files) / 1_000.0;
    let preprocess_sec = geogrid_sec + ungrib_sec + metgrid_sec + real_sec;

    let effective = f64::from(cores) * parallel_efficiency(cores);
    let wrf_sec =
        f64::from(cells) * f64::from(params.levels) * f64::from(steps) * wrf_cost_sec() / effective;

    let total_sec = download_sec + preprocess_sec + wrf_sec;
    let low_sec = (total_sec * 0.6).max(60.0);
    let high_sec = total_sec * 1.8 + 120.0;
    let memory_mb = 400 + (f64::from(cells) * f64::from(params.levels) * 0.015).round() as u32;

    let summary = format!(
        "About {}–{} min for {:.0}×{:.0} km at {} km, {} h, {}×{} cells, {} cores (~{} MB). Finer Δx costs ~Δx⁻³.",
        minutes(low_sec),
        minutes(high_sec),
        params.width_km,
        params.height_km,
        trim_dx(params.dx_km),
        params.hours,
        params.nx().saturating_sub(1),
        params.ny().saturating_sub(1),
        cores,
        memory_mb
    );

    WrfEstimate {
        width_km: params.width_km.round() as u32,
        height_km: params.height_km.round() as u32,
        area_km2: params.area_km2().round() as u32,
        dx_km: params.dx_km,
        hours: params.hours,
        cores,
        levels: params.levels,
        nx: params.nx(),
        ny: params.ny(),
        cells,
        dt_sec: dt,
        steps,
        grib_files: files,
        download_min: minutes(download_sec),
        preprocess_min: minutes(preprocess_sec),
        integrate_min: minutes(wrf_sec),
        total_min: minutes(total_sec),
        total_min_low: minutes(low_sec),
        total_min_high: minutes(high_sec),
        memory_mb,
        summary,
    }
}

fn minutes(sec: f64) -> u32 {
    (sec / 60.0).ceil().max(1.0) as u32
}

fn trim_dx(dx: f64) -> String {
    if (dx - dx.round()).abs() < 0.05 {
        format!("{}", dx.round() as i32)
    } else {
        format!("{dx:.1}")
    }
}

pub fn idle() -> Wrf {
    let params = Params::from_view(450.0, DEFAULT_HOURS, 0, 0.0, DEFAULT_LEVELS);
    Wrf {
        status: WrfStatus::Idle,
        image: env::var("OMASTORM_WRF_IMAGE").unwrap_or_default(),
        lat: 0.0,
        lon: 0.0,
        message: String::new(),
        estimate: estimate(params),
    }
}

pub fn preview(lat: f64, lon: f64, span_km: f64, hours: u32, cores: u32, dx_km: f64) -> Wrf {
    let params = Params::from_view(span_km, hours, cores, dx_km, DEFAULT_LEVELS);
    Wrf {
        status: WrfStatus::Idle,
        image: env::var("OMASTORM_WRF_IMAGE").unwrap_or_default(),
        lat,
        lon,
        message: String::new(),
        estimate: estimate(params),
    }
}

pub fn default_image() -> String {
    env::var("OMASTORM_WRF_IMAGE").unwrap_or_else(|_| "ncar/wrf_tutorial:latest".into())
}

pub fn cache_dir() -> PathBuf {
    let base = env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let mut home = PathBuf::from(env::var_os("HOME").unwrap_or_default());
            home.push(".cache");
            home
        });
    base.join("omastorm").join("wrf")
}

pub fn script_path() -> Option<PathBuf> {
    if let Ok(path) = env::var("OMASTORM_WRF_SCRIPT") {
        let p = PathBuf::from(path);
        if p.is_file() {
            return Some(p);
        }
    }
    let baked = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../scripts/wrf-forecast.sh");
    if baked.is_file() {
        return Some(baked);
    }
    if let Ok(exe) = env::current_exe() {
        for rel in ["scripts/wrf-forecast.sh", "../scripts/wrf-forecast.sh"] {
            if let Some(dir) = exe.parent() {
                let candidate = dir.join(rel);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ref_params() -> Params {
        Params {
            width_km: 450.0,
            height_km: 450.0,
            dx_km: 15.0,
            hours: 6,
            cores: 4,
            levels: 33,
        }
    }

    #[test]
    fn reference_domain_is_thirty_by_thirty_mass_points() {
        let p = ref_params();
        assert_eq!(p.nx(), 31);
        assert_eq!(p.ny(), 31);
        assert_eq!(p.cells(), 900);
        assert!((p.dt_sec() - 90.0).abs() < 0.01);
        assert_eq!(p.steps(), 240);
        assert_eq!(p.grib_files(), 3);
        assert_eq!(p.area_km2(), 202_500.0);
    }

    #[test]
    fn reference_integration_is_about_ten_minutes() {
        let e = estimate(ref_params());
        assert_eq!(e.integrate_min, 10);
        assert!(e.total_min >= 12 && e.total_min <= 20);
        assert!(e.total_min_low < e.total_min);
        assert!(e.total_min_high > e.total_min);
        assert!(e.summary.contains("450"));
        assert!(e.summary.contains("15 km"));
    }

    #[test]
    fn finer_grid_scales_near_dx_cubed() {
        let coarse = estimate(ref_params());
        let mut fine = ref_params();
        fine.dx_km = 7.5;
        let fine = estimate(fine);
        assert!(fine.cells > coarse.cells * 3);
        assert!(fine.steps > coarse.steps);
        // 2× finer Δx → ~8× (cells×4 and steps×2). Allow a wide band.
        let ratio = f64::from(fine.integrate_min) / f64::from(coarse.integrate_min);
        assert!(
            ratio > 5.0 && ratio < 12.0,
            "fine/coarse integrate ratio {ratio}"
        );
    }

    #[test]
    fn larger_area_scales_with_cell_count() {
        let small = estimate(ref_params());
        let mut wide = ref_params();
        wide.width_km = 900.0;
        wide.height_km = 900.0;
        let wide = estimate(wide);
        assert_eq!(wide.area_km2, 810_000);
        assert!(wide.cells > small.cells * 3);
        assert!(wide.integrate_min > small.integrate_min * 3);
    }

    #[test]
    fn longer_forecast_adds_steps_and_grib_files() {
        let short = estimate(ref_params());
        let mut long = ref_params();
        long.hours = 12;
        let long = estimate(long);
        assert_eq!(long.grib_files, 5);
        assert!(long.steps > short.steps);
        assert!(long.integrate_min > short.integrate_min);
        assert!(long.download_min > short.download_min);
    }

    #[test]
    fn extra_cores_cut_integration_not_download() {
        let four = estimate(ref_params());
        let mut eight = ref_params();
        eight.cores = 8;
        let eight = estimate(eight);
        assert_eq!(eight.download_min, four.download_min);
        assert!(eight.integrate_min < four.integrate_min);
        assert!(eight.integrate_min >= four.integrate_min / 3);
    }

    #[test]
    fn view_span_picks_a_runnable_spacing() {
        assert_eq!(suggest_dx_km(210.0), 3.0);
        assert_eq!(suggest_dx_km(450.0), 9.0);
        assert_eq!(suggest_dx_km(900.0), 15.0);
        let p = Params::from_view(210.0, 0, 4, 0.0, 0);
        assert_eq!(p.hours, 12);
        assert_eq!(p.dx_km, 3.0);
        assert_eq!(p.levels, 33);
    }
}

//! Spherical Lambert conformal conic, the usual mid-latitude weather-chart
//! projection (NWS / WRF CONUS sheets). Parameters are taken from the
//! selected box: standard parallels at one-sixth and five-sixths of the
//! latitude span, origin at the box centre.

const R_M: f64 = 6_371_000.0;

#[derive(Clone, Copy, Debug)]
pub struct Lambert {
    n: f64,
    f: f64,
    rho0: f64,
    lon0: f64,
}

impl Lambert {
    /// Fit a secant cone to `south..=north` / `west..=east` (degrees).
    /// A vanishing latitude span becomes a tangent cone at the centre.
    pub fn for_box(west: f64, south: f64, east: f64, north: f64) -> Self {
        let lat0 = (south + north) * 0.5;
        let lon0 = (west + east) * 0.5;
        let span = north - south;
        let (lat1, lat2) = if span < 0.2 {
            (lat0, lat0)
        } else {
            (south + span / 6.0, south + 5.0 * span / 6.0)
        };
        Self::secant(lat1, lat2, lat0, lon0)
    }

    fn secant(lat1: f64, lat2: f64, lat0: f64, lon0: f64) -> Self {
        let phi0 = lat0.to_radians();
        let phi1 = lat1.to_radians();
        let phi2 = lat2.to_radians();
        let n = if (phi1 - phi2).abs() < 1e-8 {
            phi1.sin()
        } else {
            ((phi1.cos() / phi2.cos()).ln()) / (t(phi1).ln() - t(phi2).ln())
        };
        let f = phi1.cos() * t(phi1).powf(n) / n;
        let rho0 = R_M * f * t(phi0).powf(n);
        Self {
            n,
            f,
            rho0,
            lon0: lon0.to_radians(),
        }
    }

    pub fn forward(&self, lon: f64, lat: f64) -> (f64, f64) {
        let phi = lat.to_radians();
        let theta = self.n * (lon.to_radians() - self.lon0);
        let rho = R_M * self.f * t(phi).powf(self.n);
        (rho * theta.sin(), self.rho0 - rho * theta.cos())
    }

    pub fn inverse(&self, x: f64, y: f64) -> (f64, f64) {
        let dy = self.rho0 - y;
        let rho = self.n.signum() * (x * x + dy * dy).sqrt();
        let theta = x.atan2(dy);
        let t = (rho / (R_M * self.f)).powf(1.0 / self.n);
        let phi = std::f64::consts::FRAC_PI_2 - 2.0 * t.atan();
        let lambda = self.lon0 + theta / self.n;
        (lambda.to_degrees(), phi.to_degrees())
    }
}

fn t(phi: f64) -> f64 {
    (std::f64::consts::FRAC_PI_4 - phi * 0.5).tan()
}

#[cfg(test)]
mod tests {
    use super::Lambert;

    #[test]
    fn norman_round_trips() {
        let lcc = Lambert::for_box(-98.0, 34.5, -96.5, 36.2);
        let (x, y) = lcc.forward(-97.44, 35.22);
        let (lon, lat) = lcc.inverse(x, y);
        assert!((lon + 97.44).abs() < 1e-6, "{lon}");
        assert!((lat - 35.22).abs() < 1e-6, "{lat}");
    }

    #[test]
    fn north_is_up_at_the_origin() {
        let lcc = Lambert::for_box(-98.0, 34.0, -96.0, 36.0);
        let (x0, y0) = lcc.forward(-97.0, 35.0);
        let (x1, y1) = lcc.forward(-97.0, 35.2);
        assert!((x1 - x0).abs() < (y1 - y0).abs());
        assert!(y1 > y0);
    }
}

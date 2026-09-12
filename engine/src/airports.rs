//! ICAO aerodrome identification. AWC `stationinfo` is the live table;
//! this Chile-first list is the offline fallback so codes such as SCTB
//! (Tobalaba) still resolve when the feed is quiet, and so the location
//! picker can jump to an aerodrome by ICAO.

use crate::protocol::GrametFix;

#[derive(Clone, Copy)]
pub struct Airport {
    pub icao: &'static str,
    pub iata: &'static str,
    pub name: &'static str,
    pub lat: f64,
    pub lon: f64,
    /// Dirección Meteorológica de Chile national station id, when known.
    pub dmc_id: &'static str,
}

/// Chilean aerodromes the briefing and MeteoChile WRF path name first.
pub const CHILE: &[Airport] = &[
    Airport {
        icao: "SCAR",
        iata: "ARI",
        name: "Arica / Chacalluta",
        lat: -18.3483,
        lon: -70.3389,
        dmc_id: "180005",
    },
    Airport {
        icao: "SCDA",
        iata: "IQQ",
        name: "Iquique / Diego Aracena",
        lat: -20.5353,
        lon: -70.1811,
        dmc_id: "200006",
    },
    Airport {
        icao: "SCFA",
        iata: "ANF",
        name: "Antofagasta / Cerro Moreno",
        lat: -23.4445,
        lon: -70.4451,
        dmc_id: "230001",
    },
    Airport {
        icao: "SCSE",
        iata: "LSC",
        name: "La Serena / La Florida",
        lat: -29.9162,
        lon: -71.1995,
        dmc_id: "290004",
    },
    Airport {
        icao: "SCEL",
        iata: "SCL",
        name: "Santiago / Arturo Merino Benítez",
        lat: -33.3930,
        lon: -70.7858,
        dmc_id: "330021",
    },
    Airport {
        icao: "SCTB",
        iata: "",
        name: "Santiago / Tobalaba (Eulogio Sánchez)",
        lat: -33.4560,
        lon: -70.5470,
        dmc_id: "330019",
    },
    Airport {
        icao: "SCVM",
        iata: "KNA",
        name: "Viña del Mar / Torquemada",
        lat: -32.9496,
        lon: -71.4786,
        dmc_id: "330007",
    },
    Airport {
        icao: "SCIE",
        iata: "CCP",
        name: "Concepción / Carriel Sur",
        lat: -36.7727,
        lon: -73.0631,
        dmc_id: "360019",
    },
    Airport {
        icao: "SCQP",
        iata: "ZCO",
        name: "Temuco / La Araucanía",
        lat: -38.9259,
        lon: -72.6515,
        dmc_id: "380013",
    },
    Airport {
        icao: "SCVD",
        iata: "ZAL",
        name: "Valdivia / Pichoy",
        lat: -39.6499,
        lon: -73.0861,
        dmc_id: "390006",
    },
    Airport {
        icao: "SCTE",
        iata: "PMC",
        name: "Puerto Montt / El Tepual",
        lat: -41.4389,
        lon: -73.0940,
        dmc_id: "410005",
    },
    Airport {
        icao: "SCBA",
        iata: "BBA",
        name: "Balmaceda",
        lat: -45.9161,
        lon: -71.6895,
        dmc_id: "450001",
    },
    Airport {
        icao: "SCCI",
        iata: "PUQ",
        name: "Punta Arenas / Presidente Ibáñez",
        lat: -53.0026,
        lon: -70.8546,
        dmc_id: "530005",
    },
    Airport {
        icao: "SCIP",
        iata: "IPC",
        name: "Isla de Pascua / Mataveri",
        lat: -27.1648,
        lon: -109.4218,
        dmc_id: "270001",
    },
];

pub fn lookup(icao: &str) -> Option<Airport> {
    let id = icao.trim().to_ascii_uppercase();
    CHILE.iter().copied().find(|a| a.icao == id)
}

pub fn nearest(lat: f64, lon: f64) -> Option<(Airport, f64)> {
    CHILE
        .iter()
        .copied()
        .map(|a| (a, great_circle_km(lat, lon, a.lat, a.lon)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

pub fn in_chile(lat: f64, lon: f64) -> bool {
    (-56.5..=-17.0).contains(&lat) && ((-76.0..=-66.0).contains(&lon) || lon < -108.0)
}

pub fn as_fix(airport: Airport) -> GrametFix {
    GrametFix {
        icao: airport.icao.into(),
        name: airport.name.into(),
        lat: airport.lat,
        lon: airport.lon,
    }
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

    #[test]
    fn tobalaba_is_sctb_and_nearer_than_scel_from_the_east_side() {
        let found = lookup("sctb").unwrap();
        assert_eq!(found.icao, "SCTB");
        assert!(found.name.contains("Tobalaba"));
        let here = (-33.45, -70.55);
        let sctb = great_circle_km(here.0, here.1, found.lat, found.lon);
        let scel = lookup("SCEL").unwrap();
        let scel_km = great_circle_km(here.0, here.1, scel.lat, scel.lon);
        assert!(sctb < scel_km, "{sctb} vs {scel_km}");
        let (near, _) = nearest(here.0, here.1).unwrap();
        assert_eq!(near.icao, "SCTB");
    }
}

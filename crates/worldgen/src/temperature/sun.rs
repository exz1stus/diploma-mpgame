//! Where the sun stands, seen from a place on the planet at a time of day. The temperature
//! model integrates its height over a day; clients draw shadows from it.

use std::f64::consts::TAU;

use crate::{GeoPosition, PlanetParams};

/// The direction to the sun as a unit vector in the local frame: east, north and up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SunPosition {
    pub east: f64,
    pub north: f64,
    pub up: f64,
}

impl SunPosition {
    /// `normalized_time` is the time of day in [0, 1): 0.5 is noon on the prime meridian.
    pub fn at(normalized_time: f64, pos: GeoPosition, planet: &PlanetParams) -> Self {
        // Solar declination (seasons)
        let declination = planet.axial_tilt_deg.to_radians() * (planet.orbit * TAU).sin();
        // Solar hour angle: negative before local noon, when the sun is east.
        let hour_angle = (normalized_time - 0.5) * TAU + pos.longitude;
        let (sin_lat, cos_lat) = pos.latitude.sin_cos();
        let (sin_dec, cos_dec) = declination.sin_cos();
        Self {
            east: -cos_dec * hour_angle.sin(),
            north: cos_lat * sin_dec - sin_lat * cos_dec * hour_angle.cos(),
            up: sin_lat * sin_dec + cos_lat * cos_dec * hour_angle.cos(),
        }
    }

    /// Angle above the horizon in radians; negative at night.
    pub fn elevation(self) -> f64 {
        self.up.clamp(-1.0, 1.0).asin()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::FRAC_PI_2;

    const EQUINOX: PlanetParams = PlanetParams {
        orbit: 0.0,
        axial_tilt_deg: 0.0,
    };
    const EQUATOR: GeoPosition = GeoPosition {
        latitude: 0.0,
        longitude: 0.0,
    };

    #[test]
    fn the_sun_rises_east_peaks_at_noon_and_sets_west() {
        let morning = SunPosition::at(0.3, EQUATOR, &EQUINOX);
        let noon = SunPosition::at(0.5, EQUATOR, &EQUINOX);
        let evening = SunPosition::at(0.7, EQUATOR, &EQUINOX);
        let midnight = SunPosition::at(0.0, EQUATOR, &EQUINOX);
        assert!(morning.east > 0.0 && evening.east < 0.0);
        assert!(
            (noon.elevation() - FRAC_PI_2).abs() < 1e-9,
            "overhead at the equinox"
        );
        assert!(midnight.elevation() < 0.0);
        for sun in [morning, noon, evening, midnight] {
            let length = (sun.east.powi(2) + sun.north.powi(2) + sun.up.powi(2)).sqrt();
            assert!((length - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn at_noon_the_sun_stands_toward_the_equator() {
        let north = GeoPosition {
            latitude: 0.8,
            longitude: 0.0,
        };
        let south = GeoPosition {
            latitude: -0.8,
            longitude: 0.0,
        };
        assert!(SunPosition::at(0.5, north, &EQUINOX).north < 0.0);
        assert!(SunPosition::at(0.5, south, &EQUINOX).north > 0.0);
    }
}

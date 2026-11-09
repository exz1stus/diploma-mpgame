use std::f64::consts::TAU;

use crate::{GeoPosition, NoiseSource, TilePos, WorldGeography};

#[derive(Clone, Copy, Debug)]
pub struct PlanetParams {
    /// Orbital phase in [0, 1): 0 is equinox, 0.25 northern summer, 0.75 northern winter.
    pub orbit: f64,
    /// Axial tilt in degrees, in [0, 90].
    pub axial_tilt_deg: f64,
}

impl Default for PlanetParams {
    fn default() -> Self {
        Self {
            orbit: 0.25,
            axial_tilt_deg: 5.0,
        }
    }
}

/// Daily-average climate temperature at a world position and terrain height.
/// Owns the world geography, seasonal settings, coefficients, and regional noise.
pub struct TemperatureModel {
    config: TemperatureCoefficients,
    region_noise: Box<dyn NoiseSource>,
    geography: WorldGeography,
    planet: PlanetParams,
}

// all coefficients are in Celsius
#[derive(Clone, Copy, Debug)]
pub struct TemperatureCoefficients {
    pub base: f64, //   at zero solar exposure, zero height, and zero noise
    pub region: f64,
    pub solar: f64,
    pub altitude: f64,
}

impl Default for TemperatureCoefficients {
    fn default() -> Self {
        Self {
            base: -30.0,   // °C
            solar: 150.0,  // average climate heating °C per exposure unit
            altitude: 6.5, // °C per km
            region: 10.0,  // °C
        }
    }
}

impl TemperatureModel {
    pub fn new(noise: impl NoiseSource, geography: WorldGeography, planet: PlanetParams) -> Self {
        Self::with_config(TemperatureCoefficients::default(), noise, geography, planet)
    }

    pub fn with_config(
        config: TemperatureCoefficients,
        noise: impl NoiseSource,
        geography: WorldGeography,
        planet: PlanetParams,
    ) -> Self {
        assert!(
            [config.base, config.region, config.solar, config.altitude]
                .iter()
                .all(|value| value.is_finite()),
            "temperature coefficients must be finite",
        );
        Self {
            config,
            region_noise: Box::new(noise),
            geography,
            planet,
        }
    }

    /// Daily average temperature in Celsius for the configured season.
    /// Height is measured in kilometers.
    pub fn average_at(&self, pos: TilePos, height: f64) -> f64 {
        let geo_pos = self.geography.geo(pos);
        let daily_sun = self.daily_average_sun_exposure(geo_pos, &self.planet);
        let region_noise = self.region_noise.sample(pos.x as f64, pos.y as f64);

        self.config.base + self.config.solar * daily_sun + self.config.region * region_noise
            - self.config.altitude * height
    }

    fn daily_average_sun_exposure(&self, pos: GeoPosition, planet: &PlanetParams) -> f64 {
        const SAMPLES: usize = 48;
        let mut total = 0.0;

        for i in 0..SAMPLES {
            let time = (i as f64 + 0.5) / SAMPLES as f64;

            total += self.sun_exposure(time, pos, planet);
        }

        total / SAMPLES as f64
    }

    fn sun_exposure(&self, normalized_time: f64, pos: GeoPosition, planet: &PlanetParams) -> f64 {
        // Solar declination (seasons)
        let declination = planet.axial_tilt_deg.to_radians() * (planet.orbit * TAU).sin();

        // Solar hour angle
        let hour_angle = (normalized_time - 0.5) * TAU + pos.longitude;

        // Cosine of solar zenith angle
        let cos_zenith = pos.latitude.sin() * declination.sin()
            + pos.latitude.cos() * declination.cos() * hour_angle.cos();

        cos_zenith.max(0.0)
    }
}

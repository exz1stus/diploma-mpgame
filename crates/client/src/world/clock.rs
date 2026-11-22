//! World time. The client keeps it for now (`assets/world/clock.ron`); once the server owns
//! time it will replicate [`WorldClock`] and this plugin's advancing goes away.

use bevy::prelude::*;
use serde::Deserialize;

use crate::{assets::AssetRoot, assets::read_table, schedule::ClientSet};

const BUNDLED_CLOCK: &str = include_str!("../../assets/world/clock.ron");

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClockSettings {
    /// Real seconds per day.
    pub day_length_secs: f64,
    /// Time of day at startup, in [0, 1).
    pub start: f64,
}

/// Days since the world began; the fraction is the time of day (0.5 is noon on the prime
/// meridian, as in worldgen's sun model).
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct WorldClock {
    pub days: f64,
}

impl WorldClock {
    pub fn time_of_day(self) -> f64 {
        self.days.rem_euclid(1.0)
    }
}

#[derive(Resource, Clone, Copy, Debug)]
struct ClockRate {
    days_per_sec: f64,
}

pub struct ClockPlugin;

impl Plugin for ClockPlugin {
    fn build(&self, app: &mut App) {
        let root = app.world_mut().get_resource_or_init::<AssetRoot>().clone();
        let settings: ClockSettings = read_table(&root.join("world/clock.ron"), BUNDLED_CLOCK);
        app.insert_resource(WorldClock {
            days: settings.start,
        })
        .insert_resource(ClockRate {
            days_per_sec: 1.0 / settings.day_length_secs.max(1.0),
        })
        .add_systems(Update, advance_clock.in_set(ClientSet::Simulate));
    }
}

fn advance_clock(time: Res<Time>, rate: Res<ClockRate>, mut clock: ResMut<WorldClock>) {
    clock.days += time.delta_secs_f64() * rate.days_per_sec;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_clock_parses_and_time_of_day_wraps() {
        let settings: ClockSettings = ron::from_str(BUNDLED_CLOCK).unwrap();
        assert!(settings.day_length_secs > 0.0);
        assert!((0.0..1.0).contains(&settings.start));
        assert!((WorldClock { days: 3.25 }.time_of_day() - 0.25).abs() < 1e-12);
        assert!((WorldClock { days: -0.25 }.time_of_day() - 0.75).abs() < 1e-12);
    }
}

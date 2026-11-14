//! Camera that follows the local player through a damped spring.

use bevy::prealude::*;

use crate::{game::rendering::LocalVisualPosition, networking::state::Owned};

/// Follows the local player's visual position.
///
/// The follow is a spring-damper: `smoothing` sets how quickly the camera catches up,
/// `damping` how it settles (1 = critically damped, no overshoot; below 1 it overshoots
/// and swings back; above 1 it eases in more slowly).
#[derive(Component, Clone, Debug)]
#[require(Camera2d)]
pub struct PlayerCamera {
    /// Response time in seconds; after this long ~90% of a jump is covered (critical damping).
    pub smoothing: f32,
    /// Damping ratio of the spring.
    pub damping: f32,
    /// Farther than this (respawn, teleport), the camera jumps straight to the target.
    pub snap_distance: f32,
    velocity: Vec2,
    following: bool,
}

impl Default for PlayerCamera {
    fn default() -> Self {
        Self {
            smoothing: 0.25,
            damping: 1.0,
            snap_distance: 2048.0,
            velocity: Vec2::ZERO,
            following: false,
        }
    }
}

impl PlayerCamera {
    /// Largest integration step; keeps the spring stable at low frame rates.
    const MAX_STEP: f32 = 1.0 / 240.0;

    /// Advances the camera from `position` toward `target` by `dt` seconds.
    pub fn step(&mut self, position: Vec2, target: Vec2, dt: f32) -> Vec2 {
        if !self.following
            || self.smoothing <= 0.0
            || position.distance(target) > self.snap_distance
        {
            self.following = true;
            self.velocity = Vec2::ZERO;
            return target;
        }
        let omega = 4.0 / self.smoothing;
        let steps = (dt / Self::MAX_STEP).ceil().max(1.0);
        let h = dt / steps;
        let mut position = position;
        for _ in 0..steps as u32 {
            let acceleration =
                omega * omega * (target - position) - 2.0 * self.damping * omega * self.velocity;
            self.velocity += acceleration * h;
            position += self.velocity * h;
        }
        position
    }

    /// Next step starts by jumping to the target (e.g. after reconnecting).
    pub fn reset(&mut self) {
        self.following = false;
        self.velocity = Vec2::ZERO;
    }
}

pub fn follow_player_camera(
    time: Res<Time>,
    player: Query<&LocalVisualPosition, With<Owned>>,
    mut cameras: Query<(&mut PlayerCamera, &mut Transform)>,
) {
    let target = player.iter().find_map(|visual| visual.position);
    for (mut camera, mut transform) in &mut cameras {
        let Some(target) = target else {
            camera.reset();
            continue;
        };
        let position = camera.step(transform.translation.truncate(), target, time.delta_secs());
        transform.translation = position.extend(transform.translation.z);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Simulates a 100-unit jump at 60 FPS; returns positions along the x axis.
    fn follow(camera: &mut PlayerCamera, seconds: f32) -> Vec<f32> {
        camera.step(Vec2::ZERO, Vec2::ZERO, 0.0); // start following at the origin
        let mut position = Vec2::ZERO;
        let target = Vec2::new(100.0, 0.0);
        (0..(seconds * 60.0) as usize)
            .map(|_| {
                position = camera.step(position, target, 1.0 / 60.0);
                position.x
            })
            .collect()
    }

    #[test]
    fn first_step_snaps_to_the_target() {
        let mut camera = PlayerCamera::default();
        assert_eq!(
            camera.step(Vec2::ZERO, Vec2::splat(50.0), 1.0 / 60.0),
            Vec2::splat(50.0)
        );
    }

    #[test]
    fn critical_damping_converges_without_overshoot() {
        let mut camera = PlayerCamera::default();
        let path = follow(&mut camera, 2.0);
        assert!(path.iter().all(|&x| x <= 100.0 + 1e-3), "no overshoot");
        assert!(path.windows(2).all(|w| w[1] >= w[0] - 1e-4), "monotonic");
        let at_smoothing = path[(camera.smoothing * 60.0) as usize - 1];
        assert!((85.0..95.0).contains(&at_smoothing), "{at_smoothing}");
        assert!((100.0 - path.last().unwrap()).abs() < 0.1);
    }

    #[test]
    fn low_damping_overshoots_and_settles() {
        let mut camera = PlayerCamera {
            damping: 0.3,
            ..Default::default()
        };
        let path = follow(&mut camera, 4.0);
        assert!(
            path.iter().any(|&x| x > 110.0),
            "underdamped spring overshoots"
        );
        assert!((100.0 - path.last().unwrap()).abs() < 1.0);
    }

    #[test]
    fn large_frame_times_stay_stable_and_far_targets_snap() {
        let mut camera = PlayerCamera::default();
        camera.step(Vec2::ZERO, Vec2::ZERO, 0.0);
        let position = camera.step(Vec2::ZERO, Vec2::new(100.0, 0.0), 0.5);
        assert!(position.x > 0.0 && position.x <= 100.0, "{position}");
        let far = Vec2::new(10_000.0, 0.0);
        assert_eq!(camera.step(position, far, 1.0 / 60.0), far);
    }
}

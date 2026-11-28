//! Sprite-sheet animation of entity visuals: an idle and a run clip, each drawn facing the
//! camera (front) or away from it (back). The clip and facing follow how the entity moves:
//! the local player by its motion on screen, others by their replicated velocity, so all
//! keep the last facing when they stop.

use bevy::prelude::*;
use serde::Deserialize;
use shared::components::NetworkPhysicsState;

use super::spawn::Visual;
use crate::net::state::Owned;

/// How a kind's sheet is cut into frames and played, from `visuals.ron`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntitySheet {
    /// Asset path of the sheet.
    pub image: String,
    /// Width and height of one frame in sheet pixels.
    pub frame: (u32, u32),
    /// World units per sheet pixel. World art uses `TILE_SIZE / pixels_per_tile` (2.0), so
    /// the same value draws the sheet at its true pixel size.
    pub pixel_scale: f32,
    /// Seconds each frame is shown.
    pub frame_secs: f32,
    /// On-screen speed, in world units per second, from which the entity counts as running.
    pub run_speed: f32,
    pub idle: FacingFrames,
    pub run: FacingFrames,
}

/// The frames of one clip per facing, as `(column, row)` cells of the sheet.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FacingFrames {
    pub front: Vec<(u32, u32)>,
    pub back: Vec<(u32, u32)>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Facing {
    /// Toward the camera (moving down the screen).
    #[default]
    Front,
    /// Away from the camera (moving up the screen).
    Back,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Clip {
    #[default]
    Idle,
    Run,
}

impl EntitySheet {
    /// World size of one frame, drawn at `pixel_scale` world units per sheet pixel.
    pub fn frame_world_size(&self) -> Vec2 {
        let (width, height) = self.frame;
        Vec2::new(width as f32, height as f32) * self.pixel_scale
    }

    fn frames(&self, clip: Clip, facing: Facing) -> &[(u32, u32)] {
        let clip = match clip {
            Clip::Idle => &self.idle,
            Clip::Run => &self.run,
        };
        match facing {
            Facing::Front => &clip.front,
            Facing::Back => &clip.back,
        }
    }

    /// The sheet region shown `elapsed` seconds into `clip`; `None` when the clip is empty.
    pub fn frame_rect(&self, clip: Clip, facing: Facing, elapsed: f32) -> Option<Rect> {
        let frames = self.frames(clip, facing);
        let index = frame_index(elapsed, self.frame_secs, frames.len())?;
        let (column, row) = frames[index];
        Some(self.cell_rect(column, row))
    }

    fn cell_rect(&self, column: u32, row: u32) -> Rect {
        let (width, height) = self.frame;
        let min = Vec2::new((column * width) as f32, (row * height) as f32);
        Rect::from_corners(min, min + Vec2::new(width as f32, height as f32))
    }
}

fn frame_index(elapsed: f32, frame_secs: f32, frame_count: usize) -> Option<usize> {
    if frame_count == 0 {
        return None;
    }
    if frame_secs <= 0.0 {
        return Some(0);
    }
    Some((elapsed.max(0.0) / frame_secs) as usize % frame_count)
}

/// Plays a kind's [`EntitySheet`] on its visual child.
#[derive(Component)]
pub struct SheetAnimator {
    sheet: EntitySheet,
    clip: Clip,
    facing: Facing,
    elapsed: f32,
    last_position: Option<Vec2>,
    velocity: Vec2,
}

impl SheetAnimator {
    pub fn new(sheet: EntitySheet) -> Self {
        Self {
            sheet,
            clip: Clip::default(),
            facing: Facing::default(),
            elapsed: 0.0,
            last_position: None,
            velocity: Vec2::ZERO,
        }
    }

    /// The region of the first frame, for the sprite's initial look.
    pub fn first_rect(&self) -> Option<Rect> {
        self.sheet.frame_rect(self.clip, self.facing, 0.0)
    }

    /// Advances by one frame in which the visual moved to `position`; returns the region to
    /// draw. `reported` is the entity's own velocity when it is known, which then picks the
    /// clip and facing instead of the on-screen motion.
    fn advance(&mut self, position: Vec2, reported: Option<Vec2>, dt: f32) -> Option<Rect> {
        self.track_velocity(position, dt);
        let motion = reported.unwrap_or(self.velocity);
        let clip = clip_for_speed(motion.length(), self.sheet.run_speed);
        let facing = facing_for_velocity(motion, self.facing);
        if clip != self.clip || facing != self.facing {
            self.clip = clip;
            self.facing = facing;
            self.elapsed = 0.0;
        } else {
            self.elapsed += dt.max(0.0);
        }
        self.sheet.frame_rect(self.clip, self.facing, self.elapsed)
    }

    /// Smoothed on-screen velocity, so one jittery frame does not flip the clip.
    fn track_velocity(&mut self, position: Vec2, dt: f32) {
        const HALF_LIFE: f32 = 0.05;

        let previous = self.last_position.replace(position);
        let (Some(previous), true) = (previous, dt > 0.0) else {
            return;
        };
        let raw = (position - previous) / dt;
        let fraction = 1.0 - 2.0_f32.powf(-dt / HALF_LIFE);
        self.velocity = self.velocity.lerp(raw, fraction);
    }
}

fn clip_for_speed(speed: f32, run_speed: f32) -> Clip {
    if speed >= run_speed {
        Clip::Run
    } else {
        Clip::Idle
    }
}

/// Faces the vertical direction of travel; mostly sideways or no movement keeps the last
/// facing, so small vertical corrections during a sideways run do not flip it.
fn facing_for_velocity(velocity: Vec2, previous: Facing) -> Facing {
    const MIN_VERTICAL_SPEED: f32 = 1.0;
    /// Share of the speed that must be vertical, about 17° off horizontal.
    const MIN_VERTICAL_SHARE: f32 = 0.3;

    let vertical = velocity.y.abs();
    if vertical < MIN_VERTICAL_SPEED || vertical < velocity.length() * MIN_VERTICAL_SHARE {
        return previous;
    }
    if velocity.y < 0.0 {
        Facing::Front
    } else {
        Facing::Back
    }
}

pub fn animate_entity_sheets(
    time: Res<Time>,
    roots: Query<(&Transform, Option<&NetworkPhysicsState>, Has<Owned>), Without<Visual>>,
    mut visuals: Query<(&ChildOf, &Transform, &mut SheetAnimator, &mut Sprite), With<Visual>>,
) {
    let dt = time.delta_secs();
    for (child_of, transform, mut animator, mut sprite) in &mut visuals {
        let Ok((root, state, owned)) = roots.get(child_of.parent()) else {
            continue;
        };
        let position = (root.translation + transform.translation).truncate();
        if let Some(rect) = animator.advance(position, reported_velocity(state, owned), dt) {
            sprite.rect = Some(rect);
        }
    }
}

/// The server's velocity of an entity other players control. Their visuals are extrapolated
/// ahead and pulled back when a stop arrives, so on screen they briefly move backwards on
/// every stop; the replicated velocity only decays, keeping the last direction like the local
/// player's own motion does.
fn reported_velocity(state: Option<&NetworkPhysicsState>, owned: bool) -> Option<Vec2> {
    state.filter(|_| !owned).map(|state| state.velocity)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet() -> EntitySheet {
        EntitySheet {
            image: "sheet.png".into(),
            frame: (32, 32),
            pixel_scale: 2.0,
            frame_secs: 0.1,
            run_speed: 10.0,
            idle: FacingFrames {
                front: vec![(0, 0)],
                back: vec![(1, 0)],
            },
            run: FacingFrames {
                front: vec![(0, 1), (0, 2)],
                back: vec![(1, 1), (1, 2)],
            },
        }
    }

    #[test]
    fn frames_cycle_through_the_clip() {
        let sheet = sheet();
        let first = Rect::new(0.0, 32.0, 32.0, 64.0);
        let second = Rect::new(0.0, 64.0, 32.0, 96.0);
        assert_eq!(
            sheet.frame_rect(Clip::Run, Facing::Front, 0.05),
            Some(first)
        );
        assert_eq!(
            sheet.frame_rect(Clip::Run, Facing::Front, 0.15),
            Some(second)
        );
        assert_eq!(
            sheet.frame_rect(Clip::Run, Facing::Front, 0.25),
            Some(first)
        );
        let back_idle = Rect::new(32.0, 0.0, 64.0, 32.0);
        assert_eq!(
            sheet.frame_rect(Clip::Idle, Facing::Back, 9.0),
            Some(back_idle)
        );
    }

    #[test]
    fn empty_clips_have_no_frame() {
        assert_eq!(frame_index(1.0, 0.1, 0), None);
        assert_eq!(frame_index(1.0, 0.0, 3), Some(0));
    }

    #[test]
    fn facing_follows_vertical_movement_and_keeps_it_sideways() {
        assert_eq!(
            facing_for_velocity(Vec2::new(0.0, -50.0), Facing::Back),
            Facing::Front
        );
        assert_eq!(
            facing_for_velocity(Vec2::new(0.0, 50.0), Facing::Front),
            Facing::Back
        );
        assert_eq!(
            facing_for_velocity(Vec2::new(50.0, 0.0), Facing::Back),
            Facing::Back
        );
        assert_eq!(
            facing_for_velocity(Vec2::new(600.0, -50.0), Facing::Back),
            Facing::Back
        );
        assert_eq!(
            facing_for_velocity(Vec2::new(400.0, -400.0), Facing::Back),
            Facing::Front
        );
        assert_eq!(
            facing_for_velocity(Vec2::ZERO, Facing::Front),
            Facing::Front
        );
    }

    #[test]
    fn moving_runs_and_stopping_idles() {
        let mut animator = SheetAnimator::new(sheet());
        let dt = 1.0 / 60.0;
        let mut position = Vec2::ZERO;
        for _ in 0..30 {
            position.y += 100.0 * dt;
            animator.advance(position, None, dt);
        }
        assert_eq!((animator.clip, animator.facing), (Clip::Run, Facing::Back));
        for _ in 0..60 {
            animator.advance(position, None, dt);
        }
        assert_eq!((animator.clip, animator.facing), (Clip::Idle, Facing::Back));
    }

    #[test]
    fn a_reported_velocity_keeps_the_facing_through_a_snap_back() {
        let mut animator = SheetAnimator::new(sheet());
        let dt = 1.0 / 60.0;
        let mut position = Vec2::ZERO;
        for _ in 0..30 {
            position.y -= 100.0 * dt;
            animator.advance(position, Some(Vec2::new(0.0, -100.0)), dt);
        }
        assert_eq!((animator.clip, animator.facing), (Clip::Run, Facing::Front));
        // The extrapolated visual jumps back up on the stop; the velocity only decays.
        position.y += 20.0;
        let mut velocity = Vec2::new(0.0, -100.0);
        for _ in 0..60 {
            velocity *= 0.8;
            animator.advance(position, Some(velocity), dt);
        }
        assert_eq!(
            (animator.clip, animator.facing),
            (Clip::Idle, Facing::Front)
        );
    }

    #[test]
    fn only_other_players_report_their_velocity() {
        let state = NetworkPhysicsState {
            tick: 1,
            position: Vec2::ZERO,
            velocity: Vec2::new(3.0, 4.0),
        };
        assert_eq!(reported_velocity(Some(&state), false), Some(state.velocity));
        assert_eq!(reported_velocity(Some(&state), true), None);
        assert_eq!(reported_velocity(None, false), None);
    }
}

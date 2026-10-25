use bevy::prelude::Vec2;
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug)]
pub struct PositionSample {
    pub tick: u64,
    pub x: f32,
    pub y: f32,
}

#[derive(Default)]
pub struct InterpolationClock {
    tick_rate: f64,
    latest_tick: u64,
    display_tick: f64,
    last_frame_s: f64,
}

impl InterpolationClock {
    const BUFFER_TICKS: f64 = 2.0;

    pub fn reset(&mut self, tick: u64, tick_rate: u32, now: f64) {
        *self = Self {
            tick_rate: tick_rate as f64,
            latest_tick: tick,
            display_tick: tick as f64 - Self::BUFFER_TICKS,
            last_frame_s: now,
        };
    }

    pub fn observe(&mut self, tick: u64) {
        self.latest_tick = self.latest_tick.max(tick);
    }

    pub fn advance(&mut self, now: f64) -> f64 {
        let elapsed = (now - self.last_frame_s).max(0.0);
        self.last_frame_s = now;
        let gap = self.latest_tick as f64 - Self::BUFFER_TICKS - self.display_tick;
        let speed = if gap > 1.0 {
            1.1
        } else if gap < -1.0 {
            0.9
        } else {
            1.0
        };
        self.display_tick =
            (self.display_tick + elapsed * self.tick_rate * speed).min(self.latest_tick as f64);
        self.display_tick
    }
}

#[derive(Clone, Copy, Debug)]
pub struct InterpolationSample {
    pub from: PositionSample,
    pub to: PositionSample,
    pub fraction: f32,
    pub position: Vec2,
    pub status: &'static str,
}

pub fn sample_at_tick(
    samples: &VecDeque<PositionSample>,
    display_tick: f64,
) -> Option<InterpolationSample> {
    let first = *samples.front()?;
    let held = |sample: PositionSample, status| InterpolationSample {
        from: sample,
        to: sample,
        fraction: 0.0,
        position: Vec2::new(sample.x, sample.y),
        status,
    };

    if display_tick <= first.tick as f64 {
        return Some(held(first, "Holding oldest sample"));
    }

    for (a, b) in samples.iter().zip(samples.iter().skip(1)) {
        if display_tick <= b.tick as f64 {
            let fraction =
                ((display_tick - a.tick as f64) / (b.tick - a.tick) as f64).clamp(0.0, 1.0) as f32;
            let start = Vec2::new(a.x, a.y);
            let end = Vec2::new(b.x, b.y);
            return Some(InterpolationSample {
                from: *a,
                to: *b,
                fraction,
                position: start.lerp(end, fraction),
                status: "Interpolating",
            });
        }
    }

    Some(held(
        *samples.back()?,
        "Holding newest sample (no future sample)",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_blend_and_buffer_edges() {
        let samples = VecDeque::from([
            PositionSample {
                tick: 10,
                x: 0.0,
                y: 10.0,
            },
            PositionSample {
                tick: 14,
                x: 8.0,
                y: 2.0,
            },
        ]);
        let blend = sample_at_tick(&samples, 11.0).unwrap();
        assert_eq!((blend.from.tick, blend.to.tick), (10, 14));
        assert_eq!(blend.fraction, 0.25);
        assert_eq!(blend.position, Vec2::new(2.0, 8.0));
        let before = sample_at_tick(&samples, 9.0).unwrap();
        assert_eq!((before.from.tick, before.to.tick), (10, 10));
        assert_eq!(before.position, Vec2::new(0.0, 10.0));
        let after = sample_at_tick(&samples, 15.0).unwrap();
        assert_eq!((after.from.tick, after.to.tick), (14, 14));
        assert_eq!(after.position, Vec2::new(8.0, 2.0));
        assert!(sample_at_tick(&VecDeque::new(), 11.0).is_none());
        assert_eq!(
            sample_at_tick(&VecDeque::from([samples[0]]), 11.0)
                .unwrap()
                .position,
            before.position
        );
    }
}

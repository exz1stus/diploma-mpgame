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

        let target = self.latest_tick as f64 - Self::BUFFER_TICKS;
        let gap = target - self.display_tick;

        if gap.abs() > 6.0 {
            self.display_tick = target;
        } else {
            let speed = (1.0 + 0.05 * gap).clamp(0.9, 1.1);

            self.display_tick = (self.display_tick + elapsed * self.tick_rate * speed)
                .min(self.latest_tick as f64 + 1.0);
        }

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

    let latest = *samples.back()?;
    if display_tick > latest.tick as f64 && samples.len() >= 2 {
        let previous = samples[samples.len() - 2];
        let tick_span = latest.tick.saturating_sub(previous.tick);
        if tick_span > 0 {
            let delta = Vec2::new(latest.x - previous.x, latest.y - previous.y);
            let fraction =
                ((display_tick - latest.tick as f64).clamp(0.0, 1.0) / tick_span as f64) as f32;
            return Some(InterpolationSample {
                from: previous,
                to: latest,
                fraction,
                position: Vec2::new(latest.x, latest.y) + delta * fraction,
                status: "Extrapolating",
            });
        }
    }

    Some(held(latest, "Holding newest sample (no future sample)"))
}

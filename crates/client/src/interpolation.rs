use bevy::prelude::Vec2;
use std::collections::VecDeque;

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

pub fn position_at_tick(samples: &VecDeque<PositionSample>, display_tick: f64) -> Option<Vec2> {
    let first = samples.front()?;

    if display_tick <= first.tick as f64 {
        return Some(Vec2::new(first.x, first.y));
    }

    for (a, b) in samples.iter().zip(samples.iter().skip(1)) {
        if display_tick <= b.tick as f64 {
            let fraction =
                ((display_tick - a.tick as f64) / (b.tick - a.tick) as f64).clamp(0.0, 1.0) as f32;

            let start = Vec2::new(a.x, a.y);
            let end = Vec2::new(b.x, b.y);
            return Some(start.lerp(end, fraction));
        }
    }

    let last = samples.back()?;
    Some(Vec2::new(last.x, last.y))
}

use bevy::ecs::resource::Resource;
use shared::movement::TickNumber;
pub const MAX_CATCHUP_TICKS: TickNumber = 256;

#[derive(Clone, Copy, Debug)]
struct ServerTimeSample {
    estimated_tick_at_receive: f64,
    received_at_secs: f64,

    source_tick: TickNumber,
}
#[derive(Resource, Debug)]
pub struct ClientSimulationClock {
    timestep_secs: f64,
    completed_tick: Option<TickNumber>,
    in_progress_tick: Option<TickNumber>,
    server_sample: Option<ServerTimeSample>,
    lead_ticks: u32,
}
impl Default for ClientSimulationClock {
    fn default() -> Self {
        Self {
            timestep_secs: 1.0 / 60.0,
            completed_tick: None,
            in_progress_tick: None,
            server_sample: None,
            lead_ticks: 2,
        }
    }
}
impl ClientSimulationClock {
    pub fn initialize(&mut self, baseline_tick: TickNumber, timestep_secs: f64) {
        assert!(timestep_secs.is_finite() && timestep_secs > 0.0);

        self.timestep_secs = timestep_secs;
        self.completed_tick = Some(baseline_tick);
        self.in_progress_tick = None;
    }
    //record a server-time observation
    pub fn observe_server_tick(
        &mut self,
        source_tick: TickNumber,
        received_at_secs: f64,
        estimated_sample_age_secs: f64,
    ) {
        if !received_at_secs.is_finite()
            || !estimated_sample_age_secs.is_finite()
            || estimated_sample_age_secs < 0.0
        {
            return;
        }
        if self
            .server_sample
            .is_some_and(|sample| source_tick <= sample.source_tick)
        {
            return;
        }
        let age_in_ticks = estimated_sample_age_secs / self.timestep_secs;
        let observed = source_tick as f64 + age_in_ticks;

        self.server_sample = Some(ServerTimeSample {
            estimated_tick_at_receive: self.corrected_estimate(observed, received_at_secs),
            received_at_secs,
            source_tick,
        });
    }

    /// Moves the server-tick estimate toward `observed`: forward at once, backward slowly.
    ///
    /// A sample that is processed late (a long frame while chunks stream in, a packet queued
    /// behind world data) looks like the server fell behind. Trusting it would stall
    /// prediction for a few ticks and then catch up in a burst, a visible jerk.
    /// A real slowdown is still followed within a second; a large gap is a resync.
    fn corrected_estimate(&self, observed: f64, at_secs: f64) -> f64 {
        const MAX_BACKWARD_STEP_TICKS: f64 = 0.05;
        const RESYNC_TICKS: f64 = 30.0;

        let Some(current) = self.estimated_server_tick(at_secs) else {
            return observed;
        };
        let behind = current - observed;
        if behind <= 0.0 || behind > RESYNC_TICKS {
            observed
        } else {
            current - behind.min(MAX_BACKWARD_STEP_TICKS)
        }
    }
    pub fn set_prediction_lead(&mut self, outbound_delay_secs: f64, jitter_margin_ticks: u32) {
        assert!(outbound_delay_secs.is_finite() && outbound_delay_secs >= 0.0);

        let latency_ticks = (outbound_delay_secs / self.timestep_secs).ceil() as u32;

        self.lead_ticks = latency_ticks.saturating_add(jitter_margin_ticks);
    }
    pub fn estimated_server_tick(&self, now_secs: f64) -> Option<f64> {
        if !now_secs.is_finite() {
            return None;
        }

        let sample = self.server_sample?;

        let elapsed = (now_secs - sample.received_at_secs).max(0.0);

        Some(sample.estimated_tick_at_receive + elapsed / self.timestep_secs)
    }
    pub fn target_tick(&self, now_secs: f64) -> Option<TickNumber> {
        let server_tick = self.estimated_server_tick(now_secs)?;

        Some((server_tick.floor() as TickNumber).saturating_add(self.lead_ticks as TickNumber))
    }

    pub fn completed_tick(&self) -> Option<TickNumber> {
        self.completed_tick
    }

    pub fn in_progress_tick(&self) -> Option<TickNumber> {
        self.in_progress_tick
    }

    pub fn begin_tick(&mut self, now_secs: f64) -> Option<TickNumber> {
        if self.in_progress_tick.is_some() {
            return None;
        }

        let completed = self.completed_tick?;
        let target = self.target_tick(now_secs)?;

        if completed >= target {
            return None;
        }

        let next = completed.checked_add(1)?;
        self.in_progress_tick = Some(next);

        Some(next)
    }

    /// Call only AFTER physics and history capture succeed.
    pub fn finish_tick(&mut self, tick: TickNumber) {
        assert_eq!(
            self.in_progress_tick,
            Some(tick),
            "Completed a tick that was not started"
        );

        self.completed_tick = Some(tick);
        self.in_progress_tick = None;
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f64 = 1.0 / 60.0;

    fn clock_at_tick_100() -> ClientSimulationClock {
        let mut clock = ClientSimulationClock::default();
        clock.initialize(100, DT);
        clock.observe_server_tick(100, 0.0, 0.0);
        clock
    }

    #[test]
    fn a_late_sample_barely_moves_the_clock_back() {
        let mut clock = clock_at_tick_100();
        // Tick 101 should arrive at 1 dt, but is processed 3 ticks late.
        clock.observe_server_tick(101, 4.0 * DT, 0.0);
        let estimate = clock.estimated_server_tick(4.0 * DT).unwrap();
        assert!((estimate - 103.95).abs() < 1e-9, "{estimate}");
    }

    #[test]
    fn an_early_sample_moves_the_clock_forward_at_once() {
        let mut clock = clock_at_tick_100();
        clock.observe_server_tick(105, 2.0 * DT, 0.0);
        let estimate = clock.estimated_server_tick(2.0 * DT).unwrap();
        assert!((estimate - 105.0).abs() < 1e-9, "{estimate}");
    }

    #[test]
    fn a_large_gap_resyncs() {
        let mut clock = clock_at_tick_100();
        // The server stalled for a second; follow it instead of drifting slowly.
        clock.observe_server_tick(101, 60.0 * DT, 0.0);
        let estimate = clock.estimated_server_tick(60.0 * DT).unwrap();
        assert!((estimate - 101.0).abs() < 1e-9, "{estimate}");
    }
}

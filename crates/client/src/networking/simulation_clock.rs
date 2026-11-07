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

        self.server_sample = Some(ServerTimeSample {
            estimated_tick_at_receive: source_tick as f64 + age_in_ticks,
            received_at_secs,
            source_tick,
        });
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

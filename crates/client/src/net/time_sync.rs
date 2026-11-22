use bevy::ecs::resource::Resource;

#[derive(Resource)]
pub struct TimeSyncEstimates {
    pub inbound_transport_secs: f64,
    pub outbound_transport_secs: f64,
    pub jitter_margin_ticks: u32,
}

impl Default for TimeSyncEstimates {
    fn default() -> Self {
        Self {
            inbound_transport_secs: 0.0,
            outbound_transport_secs: 0.0,
            jitter_margin_ticks: 2,
        }
    }
}

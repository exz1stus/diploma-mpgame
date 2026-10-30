use avian2d::schedule::PhysicsSystems;
use bevy::{
    app::{App, FixedPostUpdate, FixedPreUpdate, FixedUpdate, PreUpdate},
    prelude::IntoScheduleConfigs,
    time::{Fixed, Time},
};
use bevy_ecs::{resource::Resource, schedule::SystemSet, system::ResMut};
use shared::replication::ReplicationSet;

#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub(crate) enum TickSet {
    ReceiveMessages,
    DispatchMessages,
    ProcessInputs,
    Simulate,
    SyncPositions,
    QueueChanges,
    SendMessages,
}

pub(crate) fn configure(app: &mut App, tickrate: f64) {
    app.insert_resource(TickCounter(0));
    app.insert_resource(Time::<Fixed>::from_hz(tickrate));
    configure_sets(app);
    app.add_systems(
        FixedPreUpdate,
        advance_tick.before(TickSet::DispatchMessages),
    );
}

fn configure_sets(app: &mut App) {
    app.configure_sets(PreUpdate, TickSet::ReceiveMessages);
    app.configure_sets(FixedPreUpdate, TickSet::DispatchMessages);
    app.configure_sets(
        FixedUpdate,
        (TickSet::ProcessInputs, TickSet::Simulate).chain(),
    );
    app.configure_sets(
        FixedPostUpdate,
        TickSet::SyncPositions.after(PhysicsSystems::Last),
    );
    app.configure_sets(
        FixedPostUpdate,
        (
            TickSet::SyncPositions,
            ReplicationSet::Collect,
            TickSet::QueueChanges,
            TickSet::SendMessages,
        )
            .chain(),
    );
}

#[derive(Resource)]
pub(crate) struct TickCounter(pub(crate) u64);

fn advance_tick(mut tick_counter: ResMut<TickCounter>) {
    tick_counter.0 += 1;
}

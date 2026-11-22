use super::{Id, InventoryService, Result};
use bevy::prelude::World;
use bevy_renet::renet::ClientId;
use shared::inventory::{
    InventoryCommand, InventoryError as Error, InventoryOutcome as Outcome, InventoryState,
};
use std::collections::VecDeque;

struct CachedOperation {
    counter: u64,
    fingerprint: Vec<u8>,
    result: Result<Outcome>,
}

pub(super) struct Session {
    pub(super) epoch: u64,
    pub(super) player: Option<Id>,
    pub(super) open: Option<Id>,
    last_counter: u64,
    cache: VecDeque<CachedOperation>,
    tokens: f64,
    token_time: f64,
    pub(super) serial: u64,
    pub(super) last_state: Option<InventoryState>,
}

impl Session {
    pub(super) fn new(epoch: u64, player: Id, now: f64, burst: u32) -> Self {
        Self {
            epoch,
            player: Some(player),
            open: None,
            last_counter: 0,
            cache: VecDeque::new(),
            tokens: f64::from(burst),
            token_time: now,
            serial: 0,
            last_state: None,
        }
    }
}

impl InventoryService {
    pub(super) fn execute(
        &mut self,
        world: &mut World,
        client: ClientId,
        session: &mut Session,
        command: &InventoryCommand,
        now: f64,
    ) -> Result<Outcome> {
        if command.epoch != session.epoch {
            return Err(Error::WrongSession);
        }

        let fingerprint = postcard::to_allocvec(&command.action).map_err(|_| Error::InvalidItem)?;

        if command.counter <= session.last_counter {
            return match session
                .cache
                .iter()
                .find(|entry| entry.counter == command.counter)
            {
                Some(entry) if entry.fingerprint == fingerprint => entry.result,
                Some(_) => Err(Error::OperationIdReused),
                None => Err(Error::OperationTooOld),
            };
        }

        let expected = session.last_counter.checked_add(1).ok_or(Error::Overflow)?;

        if command.counter != expected {
            return Err(Error::InvalidCounter);
        }

        let elapsed = (now - session.token_time).max(0.0);

        session.tokens = (session.tokens
            + elapsed * f64::from(self.config.rules.commands_per_second))
        .min(f64::from(self.config.rules.command_burst));

        session.token_time = now;

        let result = if session.tokens < 1.0 {
            Err(Error::RateLimited)
        } else {
            session.tokens -= 1.0;
            self.apply(world, client, session, &command.action)
        };

        session.last_counter = command.counter;
        session.cache.push_back(CachedOperation {
            counter: command.counter,
            fingerprint,
            result,
        });

        while session.cache.len() > self.config.rules.cached_results_per_session {
            session.cache.pop_front();
        }
        result
    }
}

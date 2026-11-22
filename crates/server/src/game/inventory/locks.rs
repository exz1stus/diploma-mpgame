//! Short-lived slot reservations; Drop releases the complete reserved set.

use super::{Id, Result};
use shared::inventory::InventoryError as Error;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum LockKey {
    Slot(Id, u16),
    Ground(Id),
    Health(Id),
}

pub(super) struct Reservations<'a> {
    table: &'a mut BTreeSet<LockKey>,
    keys: Vec<LockKey>,
}

impl<'a> Reservations<'a> {
    pub(super) fn acquire(
        table: &'a mut BTreeSet<LockKey>,
        keys: &BTreeSet<LockKey>,
    ) -> Result<Self> {
        if keys.iter().any(|key| table.contains(key)) {
            return Err(Error::SlotBusy);
        }
        let keys: Vec<_> = keys.iter().copied().collect();
        table.extend(keys.iter().copied());

        Ok(Self { table, keys })
    }
}
impl Drop for Reservations<'_> {
    fn drop(&mut self) {
        for key in &self.keys {
            self.table.remove(key);
        }
    }
}

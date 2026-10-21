//! Per-source custody for finite consumers that do not wait for sibling commits.
//!
//! Only the ledger constructs terminal evidence. A lease names this exact ledger
//! instance and never creates a source that was not retained at registration.

use std::sync::{Arc, Weak};
use weaver_types::state_flow::SourceBlockId;

use super::{LedgerError, SourceDisposition};

#[derive(Debug)]
pub(in crate::flow) struct SourceLease {
    identity: Arc<()>,
    id: u64,
    // A dropped handle abandons only consumer interest, never selected debt.
    _life: Arc<()>,
}

#[derive(Debug)]
pub(in crate::flow) struct SourceLeaseRegistrationFailure {
    pub sources: Vec<SourceBlockId>,
    pub error: LedgerError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum LeaseSlotStatus {
    Pending,
    Ready,
    Taken,
    Released,
}

#[derive(Debug)]
struct Slot {
    source: SourceBlockId,
    status: LeaseSlotStatus,
    disposition: Option<SourceDisposition>,
}

#[derive(Debug)]
struct Registration {
    id: u64,
    life: Weak<()>,
    slots: Vec<Slot>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct LeaseSlotKey {
    id: u64,
    slot: usize,
}

pub(super) struct SourceLeases {
    identity: Arc<()>,
    next_id: Option<u64>,
    registrations: Vec<Registration>,
}

impl SourceLeases {
    pub(super) fn new() -> Self {
        Self {
            identity: Arc::new(()),
            next_id: Some(1),
            registrations: Vec::new(),
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(super) fn len(&self) -> usize {
        self.registrations
            .iter()
            .filter(|registration| registration.life.strong_count() != 0)
            .count()
    }

    pub(super) fn prune_abandoned(&mut self) {
        self.registrations
            .retain(|registration| registration.life.strong_count() != 0);
    }

    pub(super) fn retained(&self, lease: &SourceLease) -> Result<(u64, u64), LedgerError> {
        Ok(self
            .registration(lease)?
            .slots
            .iter()
            .filter_map(|slot| slot.disposition.as_ref())
            .fold((0, 0), |(blocks, bytes), disposition| {
                (blocks + 1, bytes + disposition.bytes())
            }))
    }

    pub(super) fn watches(&self, source: &SourceBlockId) -> bool {
        self.watching(source).is_some()
    }

    pub(super) fn watching(&self, source: &SourceBlockId) -> Option<LeaseSlotKey> {
        self.registrations
            .iter()
            .filter(|registration| registration.life.strong_count() != 0)
            .find_map(|registration| {
                registration
                    .slots
                    .iter()
                    .position(|slot| {
                        slot.source == *source
                            && matches!(
                                slot.status,
                                LeaseSlotStatus::Pending | LeaseSlotStatus::Ready
                            )
                    })
                    .map(|slot| LeaseSlotKey {
                        id: registration.id,
                        slot,
                    })
            })
    }

    pub(super) fn register(
        &mut self,
        sources: Vec<SourceBlockId>,
    ) -> Result<SourceLease, SourceLeaseRegistrationFailure> {
        self.prune_abandoned();
        let Some(id) = self.next_id else {
            return Err(SourceLeaseRegistrationFailure {
                sources,
                error: LedgerError("source lease identity counter exhausted"),
            });
        };
        let mut slots = Vec::new();
        if slots.try_reserve_exact(sources.len()).is_err()
            || self.registrations.try_reserve(1).is_err()
        {
            return Err(SourceLeaseRegistrationFailure {
                sources,
                error: LedgerError("source lease reservation failed"),
            });
        }
        slots.extend(sources.into_iter().map(|source| Slot {
            source,
            status: LeaseSlotStatus::Pending,
            disposition: None,
        }));
        let life = Arc::new(());
        self.registrations.push(Registration {
            id,
            life: Arc::downgrade(&life),
            slots,
        });
        self.next_id = id.checked_add(1);
        Ok(SourceLease {
            identity: Arc::clone(&self.identity),
            id,
            _life: life,
        })
    }

    fn registration(&self, lease: &SourceLease) -> Result<&Registration, LedgerError> {
        if !Arc::ptr_eq(&self.identity, &lease.identity) {
            return Err(LedgerError("source lease belongs to another ledger"));
        }
        self.registrations
            .iter()
            .find(|registration| registration.id == lease.id)
            .ok_or(LedgerError("source lease is no longer registered"))
    }

    fn registration_mut(&mut self, lease: &SourceLease) -> Result<&mut Registration, LedgerError> {
        self.prune_abandoned();
        if !Arc::ptr_eq(&self.identity, &lease.identity) {
            return Err(LedgerError("source lease belongs to another ledger"));
        }
        self.registrations
            .iter_mut()
            .find(|registration| registration.id == lease.id)
            .ok_or(LedgerError("source lease is no longer registered"))
    }

    pub(super) fn sources<'a>(
        &'a self,
        lease: &SourceLease,
    ) -> Result<impl Iterator<Item = &'a SourceBlockId>, LedgerError> {
        Ok(self
            .registration(lease)?
            .slots
            .iter()
            .map(|slot| &slot.source))
    }

    pub(super) fn status(
        &self,
        lease: &SourceLease,
        source: &SourceBlockId,
    ) -> Result<LeaseSlotStatus, LedgerError> {
        self.registration(lease)?
            .slots
            .iter()
            .find(|slot| slot.source == *source)
            .map(|slot| slot.status)
            .ok_or(LedgerError("source is outside this lease"))
    }

    pub(super) fn slot_available(&self, key: LeaseSlotKey) -> bool {
        self.registrations
            .iter()
            .find(|registration| registration.id == key.id)
            .and_then(|registration| registration.slots.get(key.slot))
            .is_some_and(|slot| {
                slot.status == LeaseSlotStatus::Pending && slot.disposition.is_none()
            })
    }

    pub(super) fn put(&mut self, key: LeaseSlotKey, disposition: SourceDisposition) {
        assert!(
            self.slot_available(key),
            "terminal lease slot cannot be replaced"
        );
        let slot = &mut self
            .registrations
            .iter_mut()
            .find(|registration| registration.id == key.id)
            .unwrap()
            .slots[key.slot];
        assert_eq!(slot.source, *disposition.source());
        slot.disposition = Some(disposition);
        slot.status = LeaseSlotStatus::Ready;
    }

    pub(super) fn take(
        &mut self,
        lease: &SourceLease,
        source: &SourceBlockId,
    ) -> Result<Option<SourceDisposition>, LedgerError> {
        let slot = self
            .registration_mut(lease)?
            .slots
            .iter_mut()
            .find(|slot| slot.source == *source)
            .ok_or(LedgerError("source is outside this lease"))?;
        match slot.status {
            LeaseSlotStatus::Pending => Ok(None),
            LeaseSlotStatus::Ready => {
                let evidence = slot
                    .disposition
                    .take()
                    .ok_or(LedgerError("ready lease lost its disposition"))?;
                slot.status = LeaseSlotStatus::Taken;
                Ok(Some(evidence))
            }
            LeaseSlotStatus::Taken => Err(LedgerError("source receipt was already taken")),
            LeaseSlotStatus::Released => Err(LedgerError("source receipt interest was released")),
        }
    }

    pub(super) fn release_source(
        &mut self,
        lease: &SourceLease,
        source: &SourceBlockId,
    ) -> Result<(), LedgerError> {
        let slot = self
            .registration_mut(lease)?
            .slots
            .iter_mut()
            .find(|slot| slot.source == *source)
            .ok_or(LedgerError("source is outside this lease"))?;
        if slot.status == LeaseSlotStatus::Taken || slot.status == LeaseSlotStatus::Released {
            return Err(LedgerError(
                "source receipt custody was already relinquished",
            ));
        }
        slot.disposition = None;
        slot.status = LeaseSlotStatus::Released;
        Ok(())
    }

    pub(super) fn release(&mut self, lease: SourceLease) -> Result<(), (SourceLease, LedgerError)> {
        self.prune_abandoned();
        if let Err(error) = self.registration(&lease) {
            return Err((lease, error));
        }
        let index = self
            .registrations
            .iter()
            .position(|registration| registration.id == lease.id)
            .unwrap();
        self.registrations.remove(index);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lease_counter_exhaustion_refuses_without_reusing_an_identity() {
        let mut leases = SourceLeases::new();
        leases.next_id = Some(u64::MAX);
        let last = leases.register(vec![]).unwrap();
        assert_eq!(last.id, u64::MAX);
        assert!(leases.register(vec![]).is_err());
        assert_eq!(leases.len(), 1);
        leases.release(last).unwrap();
        assert!(leases.register(vec![]).is_err());
        assert!(leases.is_empty());
    }

    #[test]
    fn abandoned_registration_is_not_watched_and_is_freed_on_registry_progress() {
        let mut leases = SourceLeases::new();
        let source = SourceBlockId {
            run: "run".into(),
            sequence: 1.into(),
            block: 0.into(),
        };
        let handle = leases.register(vec![source.clone()]).unwrap();
        assert!(leases.watches(&source));
        drop(handle);
        assert!(!leases.watches(&source));
        assert_eq!(leases.len(), 0);
        assert_eq!(
            leases.registrations.len(),
            1,
            "physical cleanup waits for owner progress"
        );
        leases.prune_abandoned();
        assert!(leases.registrations.is_empty());
    }
}

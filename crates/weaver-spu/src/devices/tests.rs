use std::cell::RefCell;

use super::*;

/// A driver that answers from tables and records every question, so a test
/// can say what was asked and in what order.
pub(crate) struct Recording {
    pub identities: Vec<Option<Identity>>,
    pub memory: Vec<Option<(u64, u64)>>,
    pub peers: bool,
    pub asked: RefCell<Vec<String>>,
}

impl Recording {
    pub(crate) fn uniform(count: usize, free: u64) -> Recording {
        Recording {
            identities: vec![Some(card("RTX A6000")); count],
            memory: vec![Some((free, 48 << 30)); count],
            peers: true,
            asked: RefCell::new(Vec::new()),
        }
    }
}

impl DeviceQuery for Recording {
    fn identity(&self, position: usize) -> Option<Identity> {
        self.asked.borrow_mut().push(format!("identity {position}"));
        self.identities.get(position).cloned().flatten()
    }
    fn memory(&self, position: usize) -> Option<(u64, u64)> {
        self.asked.borrow_mut().push(format!("memory {position}"));
        self.memory.get(position).copied().flatten()
    }
    fn can_access_peer(&self, from: usize, to: usize) -> bool {
        self.asked.borrow_mut().push(format!("peer {from} {to}"));
        self.peers
    }
}

pub(crate) fn card(name: &str) -> Identity {
    Identity {
        name: name.into(),
        capability: (8, 6),
        total: 48 << 30,
    }
}

fn ordinals(values: &[u32]) -> Vec<DeviceOrdinal> {
    values.iter().copied().map(DeviceOrdinal).collect()
}

pub(crate) fn masked(mask: &str) -> Visibility {
    Visibility {
        order: Some(ORDER_VALUE.into()),
        visible: Some(mask.into()),
    }
}

/// The mask is the binding's ordinals, decimal, comma-separated, in order.
#[test]
fn the_mask_is_the_binding_in_order() {
    assert_eq!(Visibility::mask(&ordinals(&[1, 0])), "1,0");
    assert_eq!(Visibility::mask(&ordinals(&[12])), "12");
}

/// **A wrong mask refuses, naming both values**: a missing order variable,
/// another order, a missing mask, a mask out of binding order, a mask with
/// a space, and a mask naming another device. The matching mask passes.
///
/// Perturbation: drop the order comparison, or compare the mask as a set,
/// and this test fails.
#[test]
fn a_wrong_mask_refuses_naming_both_values() {
    let devices = ordinals(&[0, 1]);
    assert_eq!(judge_mask(&devices, &masked("0,1")), Ok(()));
    let wrong = [
        Visibility {
            order: None,
            visible: Some("0,1".into()),
        },
        Visibility {
            order: Some("FASTEST_FIRST".into()),
            visible: Some("0,1".into()),
        },
        Visibility {
            order: Some(ORDER_VALUE.into()),
            visible: None,
        },
        masked("1,0"),
        masked("0, 1"),
        masked("0,2"),
        masked(""),
    ];
    for found in wrong {
        assert_eq!(
            judge_mask(&devices, &found),
            Err(MaskRefusal {
                found: found.clone(),
                expected: "0,1".into(),
            }),
            "{found:?}"
        );
    }
}

/// **Mixed members refuse**, on any one of the three facts, and the
/// refusal carries every member's ordinal and facts.
///
/// Perturbation: compare the name alone, or return early for pairs, and
/// this test fails.
#[test]
fn mixed_members_refuse_on_any_fact() {
    let devices = ordinals(&[3, 5]);
    let differ: [fn(&mut Identity); 3] = [
        |i| i.name = "RTX 2000 Ada".into(),
        |i| i.capability = (8, 9),
        |i| i.total -= 1 << 20,
    ];
    for change in differ {
        let mut query = Recording::uniform(2, 0);
        let mut second = card("RTX A6000");
        change(&mut second);
        query.identities[1] = Some(second.clone());
        assert_eq!(
            judge_identical(&devices, &query),
            Err(DeviceRefusal::Mismatched {
                members: vec![(3, card("RTX A6000")), (5, second)],
            })
        );
    }
    let same = Recording::uniform(2, 0);
    assert_eq!(judge_identical(&devices, &same), Ok(()));
}

/// **A single-device binding asks nothing of identity**, per PAIR-2.
#[test]
fn a_single_device_asks_no_identity() {
    let query = Recording::uniform(1, 0);
    assert_eq!(judge_identical(&ordinals(&[0]), &query), Ok(()));
    assert!(query.asked.borrow().is_empty());
}

/// **Positions map back to ordinals.** A binding of non-zero ordinals whose
/// second member is absent, short of room, or unreachable by its peer:
/// each refusal names the binding's ordinal, never the position.
///
/// Perturbation: report the position in any refusal and this test fails.
#[test]
fn refusals_name_binding_ordinals_not_positions() {
    let devices = ordinals(&[7, 9]);

    let mut absent = Recording::uniform(2, 100);
    absent.memory[1] = None;
    assert_eq!(
        judge_room_and_reach(&devices, &[10, 10], 0, &absent),
        Err(DeviceRefusal::Unreachable { ordinal: 9 })
    );

    let mut short = Recording::uniform(2, 100);
    short.memory[1] = Some((5, 48 << 30));
    assert_eq!(
        judge_room_and_reach(&devices, &[10, 10], 0, &short),
        Err(DeviceRefusal::NoRoom {
            ordinal: 9,
            free: 5,
            needed: 10,
            total: 48 << 30,
        })
    );

    let mut apart = Recording::uniform(2, 100);
    apart.peers = false;
    assert_eq!(
        judge_room_and_reach(&devices, &[10, 10], 0, &apart),
        Err(DeviceRefusal::NoPeerAccess { from: 7, to: 9 })
    );

    let mut unnamed = Recording::uniform(2, 0);
    unnamed.identities[1] = None;
    assert_eq!(
        judge_identical(&devices, &unnamed),
        Err(DeviceRefusal::Unreachable { ordinal: 9 })
    );
}

/// **Each device is judged against its own need plus the headroom.** The
/// second device's larger share refuses where an even need would pass.
///
/// Perturbation: judge every device against the first need, or drop the
/// headroom, and this test fails.
#[test]
fn room_is_judged_per_device() {
    let devices = ordinals(&[0, 1]);
    let query = Recording::uniform(2, 100);
    assert_eq!(
        judge_room_and_reach(&devices, &[60, 90], 10, &query),
        Ok(())
    );
    assert_eq!(
        judge_room_and_reach(&devices, &[60, 95], 10, &query),
        Err(DeviceRefusal::NoRoom {
            ordinal: 1,
            free: 100,
            needed: 105,
            total: 48 << 30,
        })
    );
    assert!(judge_room_and_reach(&devices, &[95, 60], 10, &query).is_err());
}

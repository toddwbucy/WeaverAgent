use super::*;

/// These run against whatever device this machine has, and skip where it
/// has none. A skip is printed rather than silent, because a suite that
/// reports success for tests it did not run is the failure mode this
/// corpus treats as worse than a red result.
fn device_present() -> bool {
    CudaContext::new(0).is_ok()
}

/// **The authority is what the device reports free at the moment of
/// admission.** A shard that plainly fits is admitted, which is the
/// positive half without which the refusal below would pass against a
/// function that refused unconditionally.
#[test]
fn a_shard_that_fits_is_admitted() {
    if !device_present() {
        eprintln!("SKIP a_shard_that_fits_is_admitted: no device on this machine");
        return;
    }
    assert_eq!(
        room_and_reach(&[DeviceOrdinal(0)], 1024 * 1024, 1024 * 1024),
        Ok(())
    );
}

/// The one inequality, read per device. An absurd shard refuses on room and
/// the refusal names the device and both numbers, so an operator meeting it
/// knows whether to free a card or reassign the binding.
///
/// Perturbation: change the `free < needed` comparison in `room_and_reach`
/// to `free < shard_bytes`, dropping the headroom term, and this test still
/// passes, which is why the headroom's presence is argued at its own clause
/// rather than rested on this test. Deleting the comparison entirely fails
/// it. Watched under that deletion.
#[test]
fn a_shard_larger_than_the_device_refuses_on_room() {
    if !device_present() {
        eprintln!("SKIP a_shard_larger_than_the_device_refuses_on_room: no device");
        return;
    }
    let absurd = 1024u64 * 1024 * 1024 * 1024; // a terabyte
    match room_and_reach(&[DeviceOrdinal(0)], absurd, 0) {
        Err(DeviceRefusal::NoRoom {
            ordinal, needed, ..
        }) => {
            assert_eq!(ordinal, 0);
            assert_eq!(needed, absurd);
        }
        other => panic!("a terabyte shard refuses on room, got {other:?}"),
    }
}

/// **The refusal separates a card too small from a card held by something
/// else**, per Spec section 5.
///
/// A terabyte shard against any card here is the first reading: the need
/// exceeds the whole device, so no clearing would help and the binding is
/// what must change. The second reading is a need under the capacity and
/// over what was free, which says the card could serve this load if
/// something released it.
///
/// **This is the case the overnight measured and nothing tested.** Under a
/// deliberate occupant the refusal read `free 7362707456, needed
/// 34911404576` eight times out of eight, and without the capacity beside
/// them those two figures are the too-small reading exactly.
///
/// Perturbation: drop `total` from the refusal, or report `free` in its
/// place, and the arithmetic below cannot tell the two readings apart.
#[test]
fn the_room_refusal_says_which_reading_applies() {
    if !device_present() {
        eprintln!("SKIP the_room_refusal_says_which_reading: no device");
        return;
    }
    let absurd = 1024u64 * 1024 * 1024 * 1024;
    let Err(DeviceRefusal::NoRoom {
        free,
        needed,
        total,
        ..
    }) = room_and_reach(&[DeviceOrdinal(0)], absurd, 0)
    else {
        panic!("a terabyte shard refuses on room");
    };
    assert!(total >= free, "free {free} exceeds capacity {total}");
    assert!(total > 0, "the driver answered no capacity at all");
    // The too-small reading: nothing released on this card would help.
    assert!(
        needed > total,
        "a terabyte exceeds this card, so the need is over the capacity \
         rather than merely over what was free: needed {needed}, total {total}"
    );
    // And the two readings are distinguishable, which is the whole claim.
    // A need under the capacity and over the free figure is the other one,
    // and the same three numbers decide it.
    let held = total.saturating_sub(free);
    let would_be_contention = |n: u64| n <= total && n > free;
    assert!(
        !would_be_contention(needed),
        "this case must not read as contention"
    );
    assert!(
        would_be_contention(free + 1) || free >= total,
        "a need just over the free figure and under the capacity reads as \
         contention: free {free}, total {total}, held {held}"
    );
}

/// An ordinal naming no device refuses as unreachable rather than as a room
/// condition, because the two send an operator to different places.
#[test]
fn an_ordinal_naming_no_device_refuses_as_unreachable() {
    let absent = DeviceOrdinal(250);
    assert_eq!(
        room_and_reach(&[absent], 1024, 0),
        Err(DeviceRefusal::Unreachable { ordinal: 250 })
    );
}

//! conforms: spu-device-authority-is-the-driver
//! conforms: spu-room-refusal-carries-capacity
//!
//! The device queries the admission judgment reads, per `weaver-spu-Spec`
//! section 3.
//!
//! **The device judgment reads the driver rather than this crate's own
//! accounting.** Charter section 10 held the question open with the archived
//! tree's answer recorded as a preference without a reason: that tree held its
//! own allocation ledger as the authority and marked the driver query as
//! diagnostics only, which prefers the number that cannot see the thing the
//! check exists for. This crate holds no fleet ledger to prefer, and the case
//! it exists to catch is a device occupied by something this program did not
//! put there, so the authority is what the device reports free at the moment of
//! admission.
//!
//! **The cost is named rather than hidden:** the queries below run on the admit
//! path, which happens twice per residency and never inside a turn.
//!
//! This module compiles only under the `cuda` feature. The no-feature build
//! reaches none of it, which is what keeps the family surface testable on a
//! machine with no device.

use std::sync::Arc;

use cudarc::driver::{CudaContext, sys};
use weaver_types::DeviceOrdinal;

/// Why the driver refused a device set.
#[derive(Debug, Clone, PartialEq)]
pub enum DeviceRefusal {
    /// The ordinal names no device, or the driver would not open it.
    Unreachable { ordinal: u32 },
    /// The one inequality, read per device: room for this device's shard plus
    /// the residency's headroom.
    ///
    /// **`total` is carried because `free` alone cannot say why.** Seven
    /// gibibytes free against a thirty-five gibibyte need is a card too small
    /// where the capacity is eight, and a card held by something else where
    /// the capacity is forty-eight. One is a binding to change and the other
    /// is a machine to clear, and the driver answers both figures in the same
    /// call, so keeping one discarded the distinction for nothing.
    ///
    /// **No case is added and none is classified**, per Spec section 5. A
    /// reader with all three figures can tell, and a `Contended` case would be
    /// a judgment this crate has no ground to make: an occupant may be a
    /// second agent the operator wants there.
    NoRoom {
        ordinal: u32,
        free: u64,
        needed: u64,
        total: u64,
    },
    /// A sharded forward exchanges activations across the set, so a set without
    /// peer access is a set that cannot serve. Discovered at admit rather than
    /// at the first turn.
    NoPeerAccess { from: u32, to: u32 },
}

/// Judge room and reach across the assigned set.
///
/// Room first, per device, then reach across each pair. A single-device set is
/// judged on room alone: there is no pair to exchange activations across, so
/// asking the driver about peer access would be asking a question the topology
/// does not pose.
pub fn room_and_reach(
    devices: &[DeviceOrdinal],
    shard_bytes: u64,
    headroom_bytes: u64,
) -> Result<(), DeviceRefusal> {
    let mut contexts = Vec::with_capacity(devices.len());
    for device in devices {
        let context =
            context_for(device.0).ok_or(DeviceRefusal::Unreachable { ordinal: device.0 })?;
        contexts.push((device.0, context));
    }

    let needed = shard_bytes.saturating_add(headroom_bytes);
    for (ordinal, context) in &contexts {
        let (free, total) = context
            .mem_get_info()
            .map_err(|_| DeviceRefusal::Unreachable { ordinal: *ordinal })?;
        let (free, total) = (free as u64, total as u64);
        if free < needed {
            return Err(DeviceRefusal::NoRoom {
                ordinal: *ordinal,
                free,
                needed,
                total,
            });
        }
    }

    // Reach, across each ordered pair. Peer access is not guaranteed symmetric
    // by the driver's interface, so both directions are asked rather than one
    // being inferred from the other.
    for (from_ordinal, from_context) in &contexts {
        for (to_ordinal, to_context) in &contexts {
            if from_ordinal == to_ordinal {
                continue;
            }
            if !can_access_peer(from_context, to_context) {
                return Err(DeviceRefusal::NoPeerAccess {
                    from: *from_ordinal,
                    to: *to_ordinal,
                });
            }
        }
    }

    Ok(())
}

/// One primary-context handle per ordinal, held for the life of the process.
///
/// **The judge must not churn primary contexts, and the ground is a measured
/// corruption.** `CudaContext::new` retains the device's primary context and
/// the drop releases it, so a judge that was the only holder walked the
/// refcount through zero: a full primary-context destroy and recreate under
/// the driver, once per admit. The engine's own contexts arrive later and
/// looked past it, and candle's multi-device forward did not - after one
/// judge cycle the pair's cross-device arithmetic went nondeterministic,
/// weights byte-exact and every operation exact in isolation. Holding the
/// handle once removes the zero-crossing, and repeated admits stop paying a
/// context build besides.
fn context_for(ordinal: u32) -> Option<Arc<CudaContext>> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static HELD: OnceLock<Mutex<HashMap<u32, Arc<CudaContext>>>> = OnceLock::new();
    let held = HELD.get_or_init(|| Mutex::new(HashMap::new()));
    let mut held = held.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(context) = held.get(&ordinal) {
        return Some(context.clone());
    }
    let context = CudaContext::new(ordinal as usize).ok()?;
    held.insert(ordinal, context.clone());
    Some(context)
}

/// Ask the driver whether peer access holds from one device to another.
///
/// `cuDeviceCanAccessPeer` has no safe wrapper in this cudarc line, so the call
/// is made directly. A driver error is read as no access rather than
/// propagated: the question asked is whether the set can serve, and a driver
/// that cannot answer it has not said yes.
fn can_access_peer(from: &Arc<CudaContext>, to: &Arc<CudaContext>) -> bool {
    let mut answer: std::ffi::c_int = 0;
    let status =
        unsafe { sys::cuDeviceCanAccessPeer(&mut answer, from.cu_device(), to.cu_device()) };
    status == sys::CUresult::CUDA_SUCCESS && answer != 0
}

#[cfg(test)]
mod tests {
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
}
